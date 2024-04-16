use async_xdp::{
    config::{LibxdpFlags, SocketConfig, UmemConfig},
    regsiter_xdp_program, FrameManager, SingleThreadRunner, SlabManager, SlabManagerConfig, Umem,
    XdpContext, XdpContextBuilder, XdpReceiveHandle, XdpSendHandle,
};
use hwaddr::HwAddr;
use packet::{
    ether::{self, Packet, Protocol},
    Builder, Packet as PacketTrait,
};
use std::{convert::TryInto, str};

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

async fn veth_to_eth(
    veth_recev_handle: &mut XdpReceiveHandle,
    eth_send_handle: &XdpSendHandle,
    self_addr: &str,
    dst_addr: &str,
) -> Result<usize, String> {
    let mut total_bytes = 0;
    let frames = veth_recev_handle.receive().await.unwrap();
    for frame in frames {
        let data = frame.data_ref();
        let origin_pkt = data.as_ref();
        let pkt = ether::Builder::default()
            .source(self_addr.parse::<HwAddr>().unwrap())
            .unwrap()
            .destination(dst_addr.parse::<HwAddr>().unwrap())
            .unwrap()
            .protocol(Protocol::Unknown(5401))
            .unwrap()
            .payload(origin_pkt)
            .unwrap()
            .build()
            .unwrap();
        total_bytes += pkt.len();
        println!("Send pkt size: {:?}", pkt.len());
        eth_send_handle.send(pkt).unwrap();
    }
    Ok(total_bytes)
}

async fn eth_to_veth(
    eth_recev_handle: &mut XdpReceiveHandle,
    veth_send_handle: &XdpSendHandle,
) -> Result<usize, String> {
    let mut total_bytes = 0;
    let frames = eth_recev_handle.receive().await.unwrap();
    for frame in frames {
        let data = frame.data_ref();
        let pkt = Packet::new(data.as_ref()).unwrap();
        let ori_pkt = pkt.payload().to_vec();
        println!("Receive pkt size: {:?}", ori_pkt.len());
        total_bytes += ori_pkt.len();
        veth_send_handle.send(ori_pkt).unwrap();
    }
    Ok(total_bytes)
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    let conf = ini::Ini::load_from_file("../config.ini").unwrap();
    let section = conf.section(Some("pingpong")).unwrap();
    let self_mac = section.get("self_mac").unwrap().to_string();
    let dst_mac = section.get("dst_mac").unwrap().to_string();

    let veth_context = create_cxt("veth1", 0, false);

    regsiter_xdp_program("../af_xdp_kern.o", "", "ens2f1").unwrap();
    let eth_conext = create_cxt("ens2f1", 0, true);

    let mut veth_receive_handle = veth_context.receive_handle().unwrap();
    let veth_send_handle = veth_context.send_handle();
    let mut eth_receive_handle = eth_conext.receive_handle().unwrap();
    let eth_send_handle = eth_conext.send_handle();

    let join1 = tokio::spawn(async move {
        let mut total_bytes = 0;
        let mut last_time = std::time::Instant::now();
        loop {
            total_bytes += veth_to_eth(
                &mut veth_receive_handle,
                &eth_send_handle,
                &self_mac,
                &dst_mac,
            )
            .await
            .unwrap();
            let now = std::time::Instant::now();
            let elaspe = now.duration_since(last_time).as_secs();
            if elaspe >= 1 {
                println!(
                    "send total_speed: {} bytes/s",
                    (total_bytes as u64) / elaspe
                );
                total_bytes = 0;
                last_time = now;
            }
        }
    });

    let join2 = tokio::spawn(async move {
        let mut total_bytes = 0;
        let mut last_time = std::time::Instant::now();
        loop {
            total_bytes += eth_to_veth(&mut eth_receive_handle, &veth_send_handle)
                .await
                .unwrap();
            let now = std::time::Instant::now();
            let elaspe = now.duration_since(last_time).as_secs();
            if elaspe >= 1 {
                println!(
                    "receive total_speed: {} bytes/s",
                    (total_bytes as u64) / elaspe
                );
                total_bytes = 0;
                last_time = now;
            }
        }
    });

    join1.await.unwrap();
    join2.await.unwrap();
}
