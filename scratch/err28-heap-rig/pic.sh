#!/bin/bash
# #ERR-27 picture: from the Chats tab, two notification taps (chat A, chat B), then Back.
# $1 = out png (…-top.png = before Back), $2/$3 = two chat root ids. BEFORE: Back shows A; AFTER: the Agent tab.
RIG=${RIG:-/tmp/exp-dcd08adb}; A=$RIG/a
$A shell am force-stop at.exponential; $A shell am start -n at.exponential/com.exponential.app.MainActivity >/dev/null; sleep 25
python3 $RIG/tap.py '^Chats$' >/dev/null; sleep 4
for id in $2 $3; do
  $A shell am start -a android.intent.action.VIEW -d "exponential://session/$id" at.exponential >/dev/null 2>&1; sleep 6
done
$A exec-out screencap -p > ${1%.png}-top.png
$A shell input keyevent KEYCODE_BACK; sleep 3
$A exec-out screencap -p > $1
