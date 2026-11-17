import sys, json, os
d = sys.argv[1]; big = json.load(open(os.environ.get('RIG', '/tmp/exp-dcd08adb') + '/seeded-ids.json'))['bigRootId']
s = [tuple(map(int, l.split()[:1] + l.split()[2:4])) for l in open(f'{d}/samples.txt') if len(l.split()) >= 4]
opens = [(int(l.split()[0]), l.split()[2]) for l in open(f'{d}/opens.txt')]
bp = [max([a for t, h, a in s if o <= t <= o + 10] or [0]) for o, i in opens if i == big]
op = [max([a for t, h, a in s if o <= t <= o + 10] or [0]) for o, i in opens if i != big]
al = sorted(a for t, h, a in s)
print(f'big-chat opens {len(bp)}: peak alloc max {max(bp)/1024:.0f} MB, median {sorted(bp)[len(bp)//2]/1024:.0f} MB; other opens median {sorted(op)[len(op)//2]/1024:.0f} MB; overall p50 {al[len(al)//2]/1024:.0f} MB, p95 {al[int(len(al)*.95)]/1024:.0f} MB, max heap size {max(h for t,h,a in s)/1024:.0f} MB')
