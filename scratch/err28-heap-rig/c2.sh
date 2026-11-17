#!/bin/bash
# C2: 10 min of chat switching via push deep links (Back after each) while the
# server writes ~1/s. $1 = out dir. Deaths, first OOM, heap samples, end dump.
RIG=${RIG:-/tmp/exp-dcd08adb}; A=$RIG/a; OUT=$1; DUR=${2:-600}; mkdir -p $OUT
D=$RIG/seeded-ids.json
BIG=$(python3 -c "import json;print(json.load(open('$D'))['bigRootId'])")
IDS=($(python3 -c "import json;print(' '.join(json.load(open('$D'))['longRootIds']))"))
[ "$($A shell getprop dalvik.vm.heapgrowthlimit | tr -d '\r')" = 256m ] || { echo "growth limit not 256m"; exit 1; }
$A shell am force-stop at.exponential; $A logcat -c
$A shell am start -n at.exponential/com.exponential.app.MainActivity >/dev/null; sleep 60
$A logcat -v epoch > $OUT/logcat.txt 2>/dev/null & LPID=$!
(source $RIG/env.sh; cd $REPO && RIG=$RIG bun $RIG/writeloop.ts $DUR > $OUT/writes.log 2>&1) & WPID=$!
START=$(date +%s); echo $START > $OUT/start
( while [ $(( $(date +%s) - START )) -lt $DUR ]; do
    P=$($A shell pidof at.exponential | tr -d '\r')
    H=$($A shell dumpsys meminfo at.exponential 2>/dev/null | grep -E "^ +Dalvik Heap" | awk '{print $8, $9}')
    echo "$(( $(date +%s) - START )) $P $H"; sleep 2
  done ) > $OUT/samples.txt & SPID=$!
i=0
while [ $(( $(date +%s) - START )) -lt $DUR ]; do
  if [ $((i % 3)) = 0 ]; then id=$BIG; else id=${IDS[$((i % ${#IDS[@]}))]}; fi
  echo "$(( $(date +%s) - START )) open $id" >> $OUT/opens.txt
  $A shell am start -a android.intent.action.VIEW -d "exponential://session/$id" at.exponential >/dev/null 2>&1
  sleep 8
  $A shell input swipe 540 700 540 1900 300; sleep 1; $A shell input swipe 540 700 540 1900 300; sleep 3
  $A shell input keyevent KEYCODE_BACK; sleep 2
  i=$((i+1))
done
wait $WPID; kill $SPID 2>/dev/null; sleep 2; kill $LPID 2>/dev/null
PID=$($A shell pidof at.exponential | tr -d '\r')
$A shell am dumpheap -g $PID /data/local/tmp/c2.hprof; sleep 10
$A shell dumpsys meminfo at.exponential | grep -E "Java Heap|Dalvik Heap|TOTAL PSS" > $OUT/end-meminfo.txt
$A pull /data/local/tmp/c2.hprof $OUT/raw.hprof >/dev/null; $A shell rm /data/local/tmp/c2.hprof
$HOME/Library/Android/sdk/platform-tools/hprof-conv -z $OUT/raw.hprof $OUT/conv.hprof && rm $OUT/raw.hprof
python3 -I $RIG/hcount.py $OUT/conv.hprof > $OUT/top.txt
python3 -I $RIG/hcount.py $OUT/conv.hprof 'ChatViewModel' IssueEntity SessionEntity 'SessionTree$Message' > $OUT/counts.txt
rm $OUT/conv.hprof
S0=$(cat $OUT/start)
echo "opens: $(wc -l < $OUT/opens.txt)  writes: $(tail -1 $OUT/writes.log)"
echo "deaths (process starts after the launch): $(( $(grep -c 'Start proc [0-9]*:at.exponential/' $OUT/logcat.txt) - 1 ))"
echo "OOM lines: $(grep -c 'OutOfMemoryError' $OUT/logcat.txt)"
grep -m1 'OutOfMemoryError' $OUT/logcat.txt | awk -v s=$S0 '{printf "first OOM at +%d s\n", $1 - s}'
echo "max Dalvik heap size/alloc (KB): $(awk '$3>m{m=$3} $4>a{a=$4} END{print m, a}' $OUT/samples.txt)"
cat $OUT/end-meminfo.txt; head -1 $OUT/top.txt; cat $OUT/counts.txt
python3 $RIG/peaks.py $OUT
