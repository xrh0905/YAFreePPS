#!/system/bin/sh
[ "$ARCH" = "arm64" ] || abort "YAFreePPS supports arm64 only."
if [ -d /data/adb/modules/FreePPS ]; then
    abort "Remove the original FreePPS and reboot before installing YAFreePPS."
fi
[ -e /sys/class/qcom-battery/pd_verifed ] || abort "Unsupported: qcom pd_verifed node missing."
[ -r /sys/class/xm_power/typec/strategy_pd_auth/adapter_svid ] || abort "Unsupported: adapter_svid unavailable."
# Preserve preferences on upgrade. No node writes or network downloads here.
for pref in free config; do
    [ -f "/data/adb/modules/YAFreePPS/$pref" ] && cp -f "/data/adb/modules/YAFreePPS/$pref" "$MODPATH/$pref"
done
set_perm_recursive "$MODPATH" 0 0 0755 0644
set_perm "$MODPATH/bin/yafreepps" 0 0 0755
ui_print "YAFreePPS: WebUI + Action; no App or broadcast forgery."
ui_print "Public PPS unlock is experimental; Xiaomi MIPPS remains kernel managed."
