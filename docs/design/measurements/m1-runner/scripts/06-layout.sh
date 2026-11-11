#!/bin/bash
# Q6: size and layout of the extracted runner (linux-x64 v2.338.2): bytes, file count, ELF files with interpreter, static or not,
# ldd of the listener, minimum glibc / GLIBCXX per native lib, shared libraries needed by name.
# Offline. Reads the pristine extraction only (nothing is run). Exit 1 if any ELF has an unresolved needed lib on this host
# (other than the musl-linked *_alpine node builds, which are expected not to resolve on glibc).
. "$(dirname "$0")/lib.sh"
start_log 06-layout
conditions; need_runner
cd "$RUNNER" || exit 2
echo "tarball bytes: $(stat -c %s "$M1/runner.tgz")"
echo "extracted bytes (du -sb, apparent): $(du -sb . | cut -f1)"
echo "extracted blocks (du -sk, on-disk KiB): $(du -sk . | cut -f1)"
echo "files: $(find . -type f | wc -l)  dirs: $(find . -type d | wc -l)  symlinks: $(find . -type l | wc -l)"
echo "--- size by top-level entry (bytes, apparent)"
for d in bin externals; do echo "$d $(du -sb $d | cut -f1)"; done
for d in externals/*; do echo "$d $(du -sb $d | cut -f1)"; done
echo "dlls in bin: $(ls bin/*.dll | wc -l); localisation dirs in bin: $(ls -d bin/*/ | wc -l)"
echo "--- ELF files (magic 7f454c46), type, interpreter, static?"
find . -type f -print0 | while IFS= read -r -d '' f; do
  head -c4 "$f" | grep -q $'^\x7fELF' || continue
  t=$(readelf -h "$f" | awk '/Type:/{print $2}')
  i=$(readelf -lW "$f" 2>/dev/null | sed -n 's/.*Requesting program interpreter: \(.*\)\]/\1/p')
  dyn=$(readelf -dW "$f" 2>/dev/null | grep -c NEEDED)
  if [ -z "$i" ] && [ "$dyn" = 0 ]; then kind=STATIC-or-no-NEEDED; else kind=dynamic; fi
  printf '%-58s type=%-5s interp=%-34s needed=%-2s %s\n' "$f" "$t" "${i:-none}" "$dyn" "$kind"
done | sort
echo "--- statically linked executables (ELF with no interpreter, no NEEDED, type EXEC or DYN with entry): see kind column above"
echo "--- ldd bin/Runner.Listener"; ldd bin/Runner.Listener
echo "--- NEEDED (soname) per native lib"
for f in bin/*.so bin/Runner.Listener bin/Runner.Worker bin/Runner.PluginHost bin/createdump externals/node24/bin/node externals/node20/bin/node; do
  echo "$f: $(readelf -dW $f | sed -n 's/.*NEEDED.*\[\(.*\)\]/\1/p' | tr '\n' ' ')"
done
echo "--- ldd for libs with unresolved entries"
unres=0
for f in bin/*.so bin/Runner.Listener bin/Runner.Worker bin/Runner.PluginHost bin/createdump externals/node24/bin/node externals/node20/bin/node; do
  m=$(ldd $f 2>&1 | grep "not found"); [ -n "$m" ] && { echo "$f: $m" | tr '\n' ' '; echo; case $f in *libcoreclrtraceptprovider*|*mscordaccore*|*mscordbi*) ;; *) unres=1;; esac; }
done
echo "--- highest GLIBC_ symbol version required, per ELF (objdump -T | grep GLIBC_ | sort -V | tail -1)"
for f in $(find . -type f | while read -r f; do head -c4 "$f" | grep -q $'^\x7fELF' && echo "$f"; done | sort); do
  echo "$f $(objdump -T "$f" 2>/dev/null | grep -o 'GLIBC_[0-9.]*' | sort -Vu | tail -1)"
done
echo "--- overall maximum GLIBC_ across all ELF files:"
for f in $(find . -type f | while read -r f; do head -c4 "$f" | grep -q $'^\x7fELF' && echo "$f"; done); do objdump -T "$f" 2>/dev/null | grep -o 'GLIBC_[0-9.]*'; done | sort -Vu | tail -1
echo "--- highest GLIBCXX_ / CXXABI_ required (libstdc++ needed by coreclr libs)"
for f in bin/*.so bin/Runner.Listener; do echo "$f $(objdump -T $f 2>/dev/null | grep -o 'GLIBCXX_[0-9.]*' | sort -Vu | tail -1) $(objdump -T $f 2>/dev/null | grep -o 'CXXABI_[0-9.]*' | sort -Vu | tail -1)"; done | awk 'NF>1'
echo "--- shared libraries opened at run time by name (dlopen strings in the shim libs)"
strings -a bin/libSystem.Security.Cryptography.Native.OpenSsl.so | grep -E '^lib(ssl|crypto)\.so' | sort -u
strings -a bin/libSystem.Globalization.Native.so | grep -E '^libicu' | sort -u | head
strings -a bin/libSystem.Net.Security.Native.so | grep -E 'gssapi|krb5' | sort -u | head
strings -a bin/libcoreclrtraceptprovider.so | grep -E '^liblttng' | sort -u
exit $unres
