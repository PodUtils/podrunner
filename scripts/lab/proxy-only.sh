#!/bin/sh
# The class proxy-only (T-284): the user of the lab reaches the network
# only through an HTTPS proxy on the loopback that intercepts TLS with its
# own CA, as on a host of a company or a sandbox of an agent. The user
# gets the proxy and a CA bundle in its environment, as such a host gives.
# Needs root for the user, the proxy and the firewall: a disposable host
# only. Put it before nonroot.sh: nonroot.sh keeps these variables.
# LAB_NO_DNS=1: the user reaches no DNS server, not even on the loopback;
# only the proxy resolves names (the host of the user QA of beta.7, T-371).
# LAB_PROXY_AUTH=1: the proxy asks for a login, which the variables of the
# proxy carry; the password is made at the start and is in no file of the
# repository.
set -eu
[ "${1:-}" = "--" ] && shift
user=${LAB_USER:-labuser}
port=${LAB_PROXY_PORT:-3128}
dir=/opt/lab-proxy
id "$user" >/dev/null 2>&1 || sudo useradd -m -s /bin/sh "$user"
id lab-proxy >/dev/null 2>&1 || sudo useradd -r -m -d "$dir" -s /usr/sbin/nologin lab-proxy

# The proxy: mitmproxy in a virtual environment of its own user.
if [ ! -x "$dir/venv/bin/mitmdump" ]; then
    sudo -u lab-proxy python3 -m venv "$dir/venv"
    sudo -u lab-proxy sh scripts/retry.sh "$dir/venv/bin/pip" install -q mitmproxy
fi
# LAB_PROXY_UPSTREAM: a host whose own egress is a proxy (a sandbox of an
# agent) chains to it, with the CA file LAB_PROXY_UPSTREAM_CA.
mode=
auth=
if [ "${LAB_PROXY_AUTH:-0}" = 1 ]; then
    pass=$(head -c 12 /dev/urandom | od -An -tx1 | tr -d ' \n')
    auth="lab:$pass"
    mode="--proxyauth $auth"
fi
if [ -n "${LAB_PROXY_UPSTREAM:-}" ]; then
    mode="$mode --mode upstream:$LAB_PROXY_UPSTREAM --set ssl_verify_upstream_trusted_ca=${LAB_PROXY_UPSTREAM_CA:-/etc/ssl/certs/ca-certificates.crt}"
fi
sudo -u lab-proxy env -u HTTPS_PROXY -u https_proxy -u HTTP_PROXY -u http_proxy \
    sh -c "cd '$dir' && nohup venv/bin/mitmdump --quiet --listen-host 127.0.0.1 --listen-port $port $mode >proxy.log 2>&1 &"
i=0
until [ -s "$dir/.mitmproxy/mitmproxy-ca-cert.pem" ] && curl -fsS -o /dev/null -x "http://${auth:+$auth@}127.0.0.1:$port" http://mitm.it/ 2>/dev/null; do
    i=$((i + 1))
    [ "$i" -le 60 ] || { echo "lab proxy-only: the proxy did not start" >&2; sudo cat "$dir/proxy.log" >&2; exit 70; }
    sleep 1
done

# The CA bundle of the host and the CA of the proxy, readable by each user.
sudo install -d -m 0755 /etc/lab-proxy
sudo sh -c "cat /etc/ssl/certs/ca-certificates.crt '$dir/.mitmproxy/mitmproxy-ca-cert.pem' > /etc/lab-proxy/ca-bundle.crt"
sudo chmod 0644 /etc/lab-proxy/ca-bundle.crt

# The firewall: the user reaches the loopback only; the proxy user goes out.
uid=$(id -u "$user")
for t in iptables ip6tables; do
    if [ "${LAB_NO_DNS:-0}" = 1 ]; then
        for proto in udp tcp; do
            sudo "$t" -C OUTPUT -m owner --uid-owner "$uid" -p "$proto" --dport 53 -j REJECT 2>/dev/null ||
                sudo "$t" -I OUTPUT -m owner --uid-owner "$uid" -p "$proto" --dport 53 -j REJECT
        done
    fi
    sudo "$t" -C OUTPUT -m owner --uid-owner "$uid" -o lo -j ACCEPT 2>/dev/null ||
        sudo "$t" -A OUTPUT -m owner --uid-owner "$uid" -o lo -j ACCEPT
    sudo "$t" -C OUTPUT -m owner --uid-owner "$uid" -j REJECT 2>/dev/null ||
        sudo "$t" -A OUTPUT -m owner --uid-owner "$uid" -j REJECT
done

proxy=http://${auth:+$auth@}127.0.0.1:$port
ca=/etc/lab-proxy/ca-bundle.crt
if [ "${LAB_NO_DNS:-0}" = 1 ] && sudo -u "$user" getent hosts github.com >/dev/null 2>&1; then
    echo "lab proxy-only: $user resolves github.com with no proxy (LAB_NO_DNS)" >&2
    exit 70
fi
if [ -n "$auth" ] && sudo -u "$user" curl -fsS -m 30 -x "http://127.0.0.1:$port" --cacert /etc/lab-proxy/ca-bundle.crt -o /dev/null https://github.com/ 2>/dev/null; then
    echo "lab proxy-only: the proxy takes a CONNECT with no login (LAB_PROXY_AUTH)" >&2
    exit 70
fi
# The checks: direct egress fails, and egress through the proxy works.
if sudo -u "$user" curl -fsS -m 10 --noproxy '*' -o /dev/null https://github.com/ 2>/dev/null; then
    echo "lab proxy-only: $user reaches github.com with no proxy" >&2
    exit 70
fi
sudo -u "$user" curl -fsS -m 30 -x "$proxy" --cacert "$ca" -o /dev/null https://github.com/ ||
    { echo "lab proxy-only: $user does not reach github.com through the proxy" >&2; exit 70; }
echo "lab proxy-only: $user reaches the network through 127.0.0.1:$port only (a CA of its own${auth:+, a login}${LAB_NO_DNS:+, no resolver})" >&2
export HTTPS_PROXY="$proxy" HTTP_PROXY="$proxy" https_proxy="$proxy" http_proxy="$proxy"
# The entry `::` is in the NO_PROXY of a sandbox of an agent; the official
# runner stops at its start on it, so the lane keeps it (T-593).
export NO_PROXY=127.0.0.1,localhost,::1,:: no_proxy=127.0.0.1,localhost,::1,::
export SSL_CERT_FILE="$ca" NODE_EXTRA_CA_CERTS="$ca" GIT_SSL_CAINFO="$ca" REQUESTS_CA_BUNDLE="$ca" CURL_CA_BUNDLE="$ca"
exec "$@"
