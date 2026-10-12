#!/bin/bash
# Build the agent VM's initramfs from THIS host's binaries: busybox, bash,
# claude (native build), git, socat, curl + their shared libs, the vsock
# kernel modules of the host kernel, the init and the in-guest test script.
# No network stack beyond loopback, no NIC driver is ever loaded (and QEMU
# gets no NIC either). No token: only a DUMMY credential.
#
# usage: build-initramfs.sh <out.cpio.gz> [kernel-release=$(uname -r)]
set -euo pipefail
out=$(readlink -f "$1")
kver=${2:-$(uname -r)}
here=$(cd "$(dirname "$0")" && pwd)
root=$(mktemp -d /tmp/sealed-initramfs.XXXXXX)
trap 'rm -rf "$root"' EXIT

mkdir -p "$root"/{bin,sbin,usr/bin,lib64,lib/x86_64-linux-gnu,lib/modules,proc,sys,dev,tmp,root,etc,run,var/tmp}
chmod 1777 "$root/tmp" "$root/var/tmp"

copy_bin() { # <host-path> <dest-path>
  install -D -m 0755 "$1" "$root$2"
  ldd "$1" 2>/dev/null | grep -oE '/[^ ]+\.so[^ ]*' | while read -r lib; do
    [ -e "$root$lib" ] || install -D -m 0755 "$(readlink -f "$lib")" "$root$lib"
  done
}

cp /usr/bin/busybox "$root/bin/busybox"   # static
for a in $("$root/bin/busybox" --list); do
  [ -e "$root/bin/$a" ] || ln -s busybox "$root/bin/$a"
done
copy_bin /bin/bash /bin/bash
copy_bin "$(readlink -f "$(command -v claude)")" /usr/bin/claude
copy_bin /usr/bin/git /usr/bin/git
copy_bin "$(readlink -f /usr/bin/socat)" /usr/bin/socat
copy_bin /usr/bin/curl /usr/bin/curl
# NSS for getpwuid etc. (git wants a user name)
for l in /lib/x86_64-linux-gnu/libnss_files.so.2; do [ -e "$l" ] && install -D -m 0755 "$l" "$root$l"; done

for m in net/vmw_vsock/vsock net/vmw_vsock/vmw_vsock_virtio_transport_common net/vmw_vsock/vmw_vsock_virtio_transport; do
  zstd -qdc "/lib/modules/$kver/kernel/$m.ko.zst" > "$root/lib/modules/$(basename "$m").ko"
done

cat > "$root/etc/passwd" <<'P'
root:x:0:0:root:/root:/bin/bash
P
echo 'root:x:0:' > "$root/etc/group"
echo 'passwd: files' > "$root/etc/nsswitch.conf"
echo '127.0.0.1 localhost' > "$root/etc/hosts"
install -m 0755 "$here/init" "$root/init"
install -m 0755 "$here/guest-tests.sh" "$root/guest-tests.sh"

(cd "$root" && find . -print0 | cpio --null -o -H newc --quiet) | gzip -1 > "$out"
ls -l "$out"
