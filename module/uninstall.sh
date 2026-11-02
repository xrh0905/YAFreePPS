#!/system/bin/sh
MODDIR=${0%/*}
# Pause first. Attached: never write 0; unplugged: restore only if still 1.
"$MODDIR/bin/yafreepps" disable
# Guard against a stale/recycled PID before signalling.
if [ -r "$MODDIR/daemon.pid" ]; then
    PID=$(cat "$MODDIR/daemon.pid")
    case "$PID" in ''|*[!0-9]*) exit 0 ;; esac
    EXE=$(readlink "/proc/$PID/exe")
    [ "$EXE" = "$MODDIR/bin/yafreepps" ] && kill -TERM "$PID"
fi
