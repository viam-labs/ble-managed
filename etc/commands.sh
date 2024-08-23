# Helpful commands for debugging; turns on kernel debug logs for l2cap_core and l2cap_sock.
echo "file net/bluetooth/l2cap_core.c +pfl" > /sys/kernel/debug/dynamic_debug/control
echo "file net/bluetooth/l2cap_sock.c +pfl" > /sys/kernel/debug/dynamic_debug/control
