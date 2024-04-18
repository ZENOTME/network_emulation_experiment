use async_xdp::{
    config::{LibxdpFlags, SocketConfig, UmemConfig},
    regsiter_xdp_program, FrameManager, PollerRunner, SingleThreadRunner, SlabManager,
    SlabManagerConfig, Umem, XdpContext, XdpContextBuilder, XdpReceiveHandle, XdpSendHandle,
};
use hwaddr::HwAddr;
use packet::ether::{Packet, Protocol};
use std::{convert::TryInto, str};

fn create_umem() -> (Umem, SlabManager) {
    let umem_config = UmemConfig::builder()
        .fill_queue_size((4096).try_into().unwrap())
        .comp_queue_size((4096).try_into().unwrap())
        .frame_headroom(16)
        .build()
        .unwrap();
    let (umem, frames) = Umem::new(umem_config, (4096 * 16).try_into().unwrap(), false).unwrap();
    let manager_config = SlabManagerConfig::new(4096);
    let frame_manager = SlabManager::new(manager_config, frames).unwrap();
    (umem, frame_manager)
}

fn create_cxt(
    if_name: &str,
    queue: u32,
    custom_xdp_prog: bool,
    runner: &impl PollerRunner,
    umem: Umem,
    frame_manager: SlabManager,
) -> XdpContext {
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
    let mut dev1_context_builder = XdpContextBuilder::new(if_name, queue);
    dev1_context_builder
        .with_socket_config(socket_config)
        .with_exist_umem(umem, frame_manager);
    dev1_context_builder.build(runner).unwrap()
}

async fn veth_to_eth(
    veth_recev_handle: &mut XdpReceiveHandle,
    eth_send_handle: &XdpSendHandle,
    self_addr: HwAddr,
    dst_addr: HwAddr,
) -> Result<usize, String> {
    let mut total_bytes = 0;
    let mut frames = veth_recev_handle.receive().await.unwrap();
    for frame in &mut frames {
        frame.adjust_head(-14);
        let mut data = frame.data_mut();
        let mut pkt = Packet::new(data.as_mut()).unwrap();
        pkt.set_destination(dst_addr)
            .unwrap()
            .set_source(self_addr)
            .unwrap()
            .set_protocol(Protocol::Unknown(5401))
            .unwrap();

        total_bytes += data.len();
    }
    eth_send_handle.send_frame(frames).unwrap();
    Ok(total_bytes)
}

async fn eth_to_veth(
    eth_recev_handle: &mut XdpReceiveHandle,
    veth_send_handle: &XdpSendHandle,
) -> Result<usize, String> {
    let mut total_bytes = 0;
    let mut frames = eth_recev_handle.receive().await.unwrap();
    for frame in &mut frames {
        frame.adjust_head(14);
        total_bytes += frame.data_ref().len();
    }
    veth_send_handle.send_frame(frames).unwrap();
    Ok(total_bytes)
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    env_logger::init();

    let conf = ini::Ini::load_from_file("../config.ini").unwrap();
    let section = conf.section(Some("pingpong")).unwrap();
    let self_mac = section
        .get("self_mac")
        .unwrap()
        .to_string()
        .parse::<HwAddr>()
        .unwrap();
    let dst_mac = section
        .get("dst_mac")
        .unwrap()
        .to_string()
        .parse::<HwAddr>()
        .unwrap();

    let runner = SingleThreadRunner::new();

    let (umem, frame_manager) = create_umem();

    let veth_context = create_cxt(
        "veth1",
        0,
        false,
        &runner,
        umem.clone(),
        frame_manager.clone(),
    );

    regsiter_xdp_program("../af_xdp_kern.o", "", "ens2f1").unwrap();
    let eth_conext = create_cxt(
        "ens2f1",
        0,
        true,
        &runner,
        umem.clone(),
        frame_manager.clone(),
    );

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
                self_mac,
                dst_mac,
            )
            .await
            .unwrap();
            let now = std::time::Instant::now();
            let elaspe = now.duration_since(last_time).as_secs();
            if elaspe >= 1 {
                log::trace!(
                    "veth -> eth total_speed: {} mbytes/s",
                    (total_bytes as u64) / elaspe / 1000 / 1000
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
                log::trace!(
                    "eth -> veth total_speed: {} mbytes/s",
                    (total_bytes as u64) / elaspe / 1000 / 1000
                );
                total_bytes = 0;
                last_time = now;
            }
        }
    });

    join1.await.unwrap();
    join2.await.unwrap();
}
