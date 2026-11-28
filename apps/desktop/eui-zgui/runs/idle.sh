#!/bin/sh
# idle.sh <binary> <mode> : real window on :0 for 16 s, CPU over seconds 5..15, RSS at 15 s. Kills only its own PID.
export DISPLAY=:0 LC_ALL=C
unset WAYLAND_DISPLAY
T0=$(date +%s%N) SPIKE_T0=$(date +%s%N) "$1" "$2" 16 2>/tmp/idle-$$.err &
pid=$!
sleep 5
read -r a < /proc/$pid/stat || exit 1
u1=$(echo "$a" | awk '{print $14+$15}')
sleep 10
read -r b < /proc/$pid/stat || exit 1
u2=$(echo "$b" | awk '{print $14+$15}')
rss=$(awk '/VmRSS/{print $2/1024}' /proc/$pid/status)
thr=$(awk '/Threads/{print $2}' /proc/$pid/status)
tck=$(getconf CLK_TCK)
echo "{\"bin\":\"$(basename $1)\",\"cpu_pct\":$(echo "($u2-$u1)*100/$tck/10" | bc -l | cut -c1-6),\"rss_mb\":$rss,\"threads\":$thr,\"first\":\"$(grep -o 'first_[a-z_]*ms.*' /tmp/idle-$$.err | head -1)\"}"
wait $pid 2>/dev/null
cat /tmp/idle-$$.err | head -5 >&2; rm -f /tmp/idle-$$.err
