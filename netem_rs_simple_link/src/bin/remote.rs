#![feature(coroutines)]
#![feature(impl_trait_in_assoc_type)]

use std::collections::HashMap;

use log::{error, trace};
use netem_rs::{Actor, ActorContext, DataView, HostAddr, MetaClient, NodeInfo, RemtoeRuntime};
use packet::ether::Packet;

#[derive(Clone)]
struct EmptyDataView;

impl DataView for EmptyDataView {
    fn new() -> Self {
        Self
    }
}

struct ForwardActor {
    context: ActorContext<EmptyDataView>,
}

impl ForwardActor {
    fn new(context: ActorContext<EmptyDataView>) -> Self {
        Self { context }
    }
}

impl Actor for ForwardActor {
    type C = EmptyDataView;

    fn new(context: ActorContext<Self::C>) -> Self {
        Self::new(context)
    }

    async fn run(&mut self) -> anyhow::Result<()> {
        let self_port_id = self.context.receive_handle.port_id();
        loop {
            let frames = self.context.receive_handle.receive_frames().await?;
            for frame in frames {
                let packet = Packet::new(frame.data_ref()).unwrap();
                if packet.destination().is_broadcast() {
                    trace!("broadcast");
                    self.context
                        .port_table
                        .for_each_port(|&port_id, send_handle| {
                            if port_id != self_port_id {
                                send_handle.send_raw_data(frame.data_ref().to_vec())
                            } else {
                                Ok(())
                            }
                        })
                        .await?;
                } else {
                    if let Some(handle) = self
                        .context
                        .port_table
                        .get_send_handle(packet.destination())
                        .await
                    {
                        handle.send_frame(smallvec::smallvec![frame])?;
                    } else {
                        error!("no send handle for {:?}", packet);
                    }
                }
            }
        }
    }
}

// MockMetaClient is a mock implementation of MetaClient which return the predefine info.
pub struct MockMetaClient {
    inner: HashMap<HostAddr, Vec<NodeInfo>>,
}

impl MetaClient for MockMetaClient {
    fn connet(_meta_addr: HostAddr) -> Self {
        MockMetaClient {
            inner: HashMap::from([
                (
                    HostAddr {
                        host: "10.0.0.44".to_string(),
                        port: 10000,
                    },
                    vec![],
                ),
                (
                    HostAddr {
                        host: "10.0.0.45".to_string(),
                        port: 10000,
                    },
                    vec![NodeInfo {
                        addr: HostAddr {
                            host: "10.0.0.44".to_string(),
                            port: 10000,
                        },
                        eth_mac_addr: Some("9c:69:b4:61:c0:b1".parse().unwrap()),
                        xdp_subnet_id: 1,
                    }],
                ),
            ]),
        }
    }

    async fn register(&self, addr: HostAddr) -> Vec<NodeInfo> {
        self.inner.get(&addr).unwrap().clone()
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();
    RemtoeRuntime::start::<ForwardActor, MockMetaClient>()
        .await
        .unwrap();
}
