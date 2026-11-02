#!/system/bin/sh
MODDIR=${0%/*}
# Start as soon as root's late-start service is available, including already-attached cables.
if [ -e "$MODDIR/disable" ] || [ -e "$MODDIR/remove" ]; then
    exit 0
fi
nohup "$MODDIR/bin/yafreepps" run >> "$MODDIR/daemon.log" 2>&1 &
