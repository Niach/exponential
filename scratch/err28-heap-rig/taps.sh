#!/bin/bash
# N notification-style deep-link taps (no Back between), then one Back + a dump: $1 = N, $2 = out dir
RIG=${RIG:-/tmp/exp-dcd08adb}; A=$RIG/a; N=$1; OUT=$2; mkdir -p $OUT
IDS=($(python3 -c "import json;d=json.load(open('$RIG/seeded-ids.json'));print(' '.join(d['longRootIds']))"))
$A shell am force-stop at.exponential; $A shell am start -n at.exponential/com.exponential.app.MainActivity >/dev/null; sleep 40
for i in $(seq 0 $((N-1))); do
  $A shell am start -a android.intent.action.VIEW -d "exponential://session/${IDS[$((i % ${#IDS[@]}))]}" at.exponential >/dev/null 2>&1
  sleep 5
done
$A shell input keyevent KEYCODE_BACK; sleep 3
PID=$($A shell pidof at.exponential | tr -d '\r')
$A shell dumpsys meminfo at.exponential | grep -E "Java Heap|TOTAL PSS" > $OUT/meminfo.txt
$A shell am dumpheap -g $PID /data/local/tmp/taps.hprof; sleep 8
$A pull /data/local/tmp/taps.hprof $OUT/raw.hprof >/dev/null; $A shell rm /data/local/tmp/taps.hprof
$HOME/Library/Android/sdk/platform-tools/hprof-conv -z $OUT/raw.hprof $OUT/conv.hprof && rm $OUT/raw.hprof
python3 -I $RIG/hcount.py $OUT/conv.hprof ChatViewModel IssueEntity 'SessionTree$Message' > $OUT/counts.txt; rm $OUT/conv.hprof
cat $OUT/meminfo.txt $OUT/counts.txt
