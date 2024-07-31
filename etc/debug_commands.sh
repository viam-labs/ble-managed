# Commands to help debug l2cap issues.
echo "file net/bluetooth/l2cap_core.c +pfl" > /sys/kernel/debug/dynamic_debug/control
echo "file net/bluetooth/l2cap_sock.c +pfl" > /sys/kernel/debug/dynamic_debug/control
