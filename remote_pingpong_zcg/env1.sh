
#
#/bin/bash

_print_help() {
    echo "This is a script to setup/tear down the test enviroment"
    echo "env up --- set up the env"
    echo "evn down --- tear down the env"
}

up() {
    sudo ip link add veth0 type veth peer name veth1
    sudo ip netns add vnet0
    sudo ifconfig veth0 hw ether aa:00:00:00:00:00
    sudo ip link set veth0 netns vnet0
    sudo ip -n vnet0 link set veth0 up
    sudo ip link set veth1 up
    sudo ip -n vnet0 addr add 10.0.0.1/24 dev veth0

    # off the rx check
    sudo ip netns exec vnet0 ethtool --offload veth0 rx off tx off
}

down() {
    # down env
    sudo ip -n vnet0 link set veth0 down
    sudo ip link set veth1 down

    # rm interface
    sudo ip link del veth1

    # rm netns
    sudo ip netns del vnet0
}



_main() {
    case $1 in
    help | --help | -h)
        _print_help
        exit
        ;;
    -*)
        echo "invalid option \`$1\`"
        exit
        ;;
    *)
        $@
        exit
        ;;
    esac

    _print_help
    exit 1
}

_main $@ 
