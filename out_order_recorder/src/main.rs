use std::{panic, process::exit, sync::Mutex, vec};

use async_xdp::{
    config::{LibxdpFlags, SocketConfig, UmemConfig}, regsiter_xdp_program, FrameManager, SingleThreadRunner, SlabManager, SlabManagerConfig, Umem, XdpContext, XdpContextBuilder
};
use clap::Parser;
use hwaddr::HwAddr;
use once_cell::sync::Lazy;
use packet::{
    ether::{self, Packet, Protocol},
    Builder, Packet as PacketTrait,
};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long, default_value = "../config.ini")]
    config_file: String,

    #[arg(short)]
    server: bool,

    #[arg(short)]
    client: bool,

    #[arg(long, default_value_t = 1000)]
    count: u32,

    #[arg(short, long, default_value_t = 16)]
    pkt_size: u8,
}

fn create_cxt(if_name: &str, queue: u32, custom_xdp_prog: bool) -> XdpContext {
    let umem_config = UmemConfig::builder()
        .fill_queue_size((4096).try_into().unwrap())
        .comp_queue_size((4096).try_into().unwrap())
        .build()
        .unwrap();
    let socket_config = if custom_xdp_prog {
        SocketConfig::builder()
            .rx_queue_size((4096).try_into().unwrap())
            .tx_queue_size((4096).try_into().unwrap())
            .libbpf_flags(LibxdpFlags::XSK_LIBXDP_FLAGS_INHIBIT_PROG_LOAD)
            .build()
    } else {
        SocketConfig::builder()
            .rx_queue_size((4096).try_into().unwrap())
            .tx_queue_size((4096).try_into().unwrap())
            .build()
    };
    let (umem, frames) = Umem::new(umem_config, (4096 * 16).try_into().unwrap(), false).unwrap();

    let manager_config = SlabManagerConfig::new(4096);
    let frame_manager = SlabManager::new(manager_config, frames).unwrap();

    let runner = SingleThreadRunner::new();

    let mut dev1_context_builder = XdpContextBuilder::new(if_name, queue);
    dev1_context_builder
        .with_socket_config(socket_config)
        .with_exist_umem(umem.clone(), frame_manager.clone());
    dev1_context_builder.build(&runner).unwrap()
}

static PKT_RECORD: Lazy<Mutex<Vec<usize>>> = Lazy::new(|| Mutex::new(Vec::new()));

async fn server(_count: u32, pkt_size: u8) {
    panic::set_hook(Box::new(|_panic_info| {
        println!("Receive record len: {}", PKT_RECORD.lock().unwrap().len());
        println!(
            "Receive record: {:?}",
            PKT_RECORD
                .lock()
                .unwrap()
                .iter()
                .enumerate()
                .collect::<Vec<_>>()
        );
        exit(0);
    }));

    let context = create_cxt("ens2f1", 0, true);
    let mut recv_handle = context.receive_handle().unwrap();
    loop {
        let frames = recv_handle.receive().await.unwrap();
        for frame in frames {
            let data = frame.data_ref();
            let pkt = Packet::new(data).unwrap();
            assert!(pkt.payload().len() == pkt_size as usize);
            let id = pkt.payload()[0];
            PKT_RECORD.lock().unwrap().push(id as usize);
        }
    }
}

async fn client(count: u32, pkt_size: u8, conf: ini::Ini) {
    let section = conf.section(Some("pingpong")).unwrap();
    let self_mac = section.get("self_mac").unwrap();
    let dst_mac = section.get("dst_mac").unwrap();

    let context = create_cxt("ens2f1", 0, true);
    let send_handle = context.send_handle();
    for i in 0..count {
        let pkt = ether::Builder::default()
            .source(self_mac.parse::<HwAddr>().unwrap())
            .unwrap()
            .destination(dst_mac.parse::<HwAddr>().unwrap())
            .unwrap()
            .protocol(Protocol::Unknown(5401))
            .unwrap()
            .payload(&vec![i as u8; pkt_size as usize])
            .unwrap()
            .build()
            .unwrap();
        send_handle.send(pkt).unwrap();
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    regsiter_xdp_program("../af_xdp_kern.o", "", "ens2f1").unwrap();

    let conf = ini::Ini::load_from_file(args.config_file).unwrap();

    if args.server && !args.client {
        server(args.count, args.pkt_size).await;
    } else if args.client && !args.server {
        client(args.count, args.pkt_size, conf).await;
    } else {
        println!("Please specify either server or client mode");
    }
}
