# Instances + shallow bytes per class in a (hprof-conv'd) HPROF dump.
# python3 -I hcount.py dump.hprof [substring ...]
import sys, struct, mmap
f = open(sys.argv[1], 'rb'); m = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
p = m.find(b'\0') + 1
ids = struct.unpack('>I', m[p:p+4])[0]; p += 12
idf = '>I' if ids == 4 else '>Q'
def rid(o): return struct.unpack(idf, m[o:o+ids])[0]
TS = {2: ids, 4: 1, 5: 2, 6: 4, 7: 8, 8: 1, 9: 2, 10: 4, 11: 8}
strings, cname, count, size = {}, {}, {}, {}
n = len(m)
while p < n:
    tag = m[p]; ln = struct.unpack('>I', m[p+5:p+9])[0]; body = p + 9; end = body + ln; p = end
    if tag == 0x01: strings[rid(body)] = m[body+ids:end].decode('utf8', 'replace')
    elif tag == 0x02: cname[rid(body+4)] = rid(body+8+ids)
    elif tag in (0x0C, 0x1C):
        q = body
        while q < end:
            st = m[q]; q += 1
            if st == 0xFF: q += ids
            elif st == 0x01: q += 2*ids
            elif st in (0x02, 0x03): q += ids + 8
            elif st == 0x04: q += ids + 4
            elif st == 0x05: q += ids
            elif st == 0x06: q += ids + 4
            elif st == 0x07: q += ids
            elif st == 0x08: q += ids + 8
            elif st == 0x20:
                q += ids + 4 + 6*ids + 4
                c = struct.unpack('>H', m[q:q+2])[0]; q += 2
                for _ in range(c): t = m[q+2]; q += 3 + TS[t]
                c = struct.unpack('>H', m[q:q+2])[0]; q += 2
                for _ in range(c): t = m[q+ids]; q += ids + 1 + TS[t]
                c = struct.unpack('>H', m[q:q+2])[0]; q += 2 + c*(ids+1)
            elif st == 0x21:
                cid = rid(q+ids+4); l = struct.unpack('>I', m[q+2*ids+4:q+2*ids+8])[0]
                count[cid] = count.get(cid, 0) + 1; size[cid] = size.get(cid, 0) + l
                q += 2*ids + 8 + l
            elif st == 0x22:
                k = struct.unpack('>I', m[q+ids+4:q+ids+8])[0]; key = ('arr', rid(q+ids+8))
                count[key] = count.get(key, 0) + 1; size[key] = size.get(key, 0) + k*ids
                q += 2*ids + 8 + k*ids
            elif st == 0x23:
                k = struct.unpack('>I', m[q+ids+4:q+ids+8])[0]; t = m[q+ids+8]; key = ('prim', t)
                count[key] = count.get(key, 0) + 1; size[key] = size.get(key, 0) + k*TS[t]
                q += ids + 9 + k*TS[t]
            else: raise SystemExit(f'unknown subtag {st:#x} at {q}')
def name(k):
    if isinstance(k, tuple):
        return f'prim[{k[1]}][]' if k[0] == 'prim' else strings.get(cname.get(k[1]), '?') + '[]'
    return strings.get(cname.get(k), '?')
rows = sorted(((size[k], count[k], name(k)) for k in count), reverse=True)
print(f'total {sum(size.values())/1e6:.1f} MB shallow, {sum(count.values())} objects')
pats = sys.argv[2:]
for s, c, nm in rows if pats else rows[:25]:
    if not pats or any(x in nm for x in pats): print(f'{s/1e6:9.2f} MB {c:9d}  {nm}')
