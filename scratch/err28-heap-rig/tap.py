# Tap a node by text / content-desc / resource-id regex (setup only: uiautomator pins the a11y cache).
import subprocess, re, sys, os
A = os.environ.get('RIG', '/tmp/exp-dcd08adb') + '/a'
def dump():
    subprocess.run([A, 'shell', 'uiautomator', 'dump', '/sdcard/u.xml'], capture_output=True)
    return subprocess.run([A, 'shell', 'cat', '/sdcard/u.xml'], capture_output=True, text=True).stdout
def find(pat):
    for m in re.finditer(r'<node [^>]*>', dump()):
        n = m.group(0)
        if any(re.search(pat, re.search(k + r'="([^"]*)"', n).group(1)) for k in ('text', 'content-desc', 'resource-id')):
            b = list(map(int, re.findall(r'\d+', re.search(r'bounds="([^"]*)"', n).group(1))))
            return (b[0] + b[2]) // 2, (b[1] + b[3]) // 2
if sys.argv[1] == 'list':
    print('\n'.join(f'{a!r} {b!r} {c!r}' for a, b, c in re.findall(r'text="([^"]*)"[^>]*resource-id="([^"]*)"[^>]*content-desc="([^"]*)"', dump()) if a or c))
else:
    p = find(sys.argv[1])
    if not p: print('NOTFOUND'); sys.exit(1)
    subprocess.run([A, 'shell', 'input', 'tap', str(p[0]), str(p[1])]); print('tapped', p)
