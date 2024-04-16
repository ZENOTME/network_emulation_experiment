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

const SELF_ADDR: &str = "9c:69:b4:61:9b:8d";
const DST_ADDR: &str = "9c:69:b4:61:9b:8d";

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
) -> Result<(), String> {
    let frames = veth_recev_handle.receive().await.unwrap();
    for frame in frames {
        let data = frame.data_ref();
        let origin_pkt = data.as_ref();
        let pkt = ether::Builder::default()
            .source(SELF_ADDR.parse::<HwAddr>().unwrap())
            .unwrap()
            .destination(DST_ADDR.parse::<HwAddr>().unwrap())
            .unwrap()
            .protocol(Protocol::Unknown(5401))
            .unwrap()
            .payload(origin_pkt)
            .unwrap()
            .build()
            .unwrap();
        eth_send_handle.send(pkt).unwrap();
    }
    Ok(())
}

async fn eth_to_veth(
    eth_recev_handle: &mut XdpReceiveHandle,
    veth_send_handle: &XdpSendHandle,
) -> Result<(), String> {
    let frames = eth_recev_handle.receive().await.unwrap();
    for frame in frames {
        let data = frame.data_ref();
        let pkt = Packet::new(data.as_ref()).unwrap();
        let ori_pkt = pkt.payload().to_vec();
        veth_send_handle.send(ori_pkt).unwrap();
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    let veth_context = create_cxt("veth0", 0, false);

    regsiter_xdp_program("af_xdp_kern.o", "", "ens2f1").unwrap();
    let eth_conext = create_cxt("ens2f1", 0, true);

    let mut veth_receive_handle = veth_context.receive_handle().unwrap();
    let veth_send_handle = veth_context.send_handle();
    let mut eth_receive_handle = eth_conext.receive_handle().unwrap();
    let eth_send_handle = eth_conext.send_handle();

    tokio::spawn(async move {
        loop {
            veth_to_eth(&mut veth_receive_handle, &eth_send_handle).await.unwrap();
        }
    });

    tokio::spawn(async move {
        loop {
            eth_to_veth(&mut eth_receive_handle, &veth_send_handle).await.unwrap();
        }
    });
}