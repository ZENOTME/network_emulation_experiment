This test used to bench the simple link(1:1) of netem_rs.


## Local
test 1-1 link in local mode.

Use `./local.sh up` to create the env with 2 netns with 2 ip: 10.0.0.1, 10.0.0.2

And then use the iperf to test the throughoutput. e.g.
```
sudo ip netns exec vnet1 iperf -s -i 1
sudo ip netns exec vnet0 iperf -c 10.0.0.2 -i 1
```

Use `./local.sh down` to clean the env

## Remote grpc
test 1-1 link in remote gprc mode.

Use `./remote_grpc_1.sh` to create a env with 1 netns with 1 ip 10.0.0.1

Use `./remote_grpc_2.sh` to create a env with 1 netns with 1 ip 10.0.0.2 in other node

And then use the iperf between two node to test the throughoutput. e.g.
```
// node2
sudo ip netns exec vnet0 iperf -s -i 1
// node1
sudo ip netns exec vnet0 iperf -c 10.0.0.2 -i 1
```

Use `./remote_grpc_*.sh clean` to clean the env.

## Remote xdp

test 1-1 link in remote gprc mode.

Use `./remote_xdp_1.sh` to create a env with 1 netns with 1 ip 10.0.0.1

Use `./remote_xdp_2.sh` to create a env with 1 netns with 1 ip 10.0.0.2 in other node

And then use the iperf between two node to test the throughoutput.e.g.
```
// node2
sudo ip netns exec vnet0 iperf -s -i 1
// node1
sudo ip netns exec vnet0 iperf -c 10.0.0.2 -i 1
```

Use `./remote_xdp_*.sh clean` to clean the env.