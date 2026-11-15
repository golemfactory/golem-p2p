#!/usr/bin/env bash
# Runs on the build machine (master) after campaign 5: the four real-world
# scenarios named *-ufw again, with the inbound filter campaign 4 had from
# ufw (default deny incoming, replies to outgoing traffic allowed) emulated
# for one fixed UDP port. Campaign 4 ran on this host's previous install with
# ufw active; since the reinstall on 2026-11-11 there is no host firewall, so
# campaign 5's *-ufw scenarios ran on an open random port. This separates the
# firewall from the iroh 1.4.1 -> 1.5.0 change.
#
# The local node binds 0.0.0.0:$PORT (instead of a random port) and an
# nftables table of its own (inet gpl_ufw_emul, removed on exit) drops new
# inbound UDP to that port: packets that answer something the node sent
# (conntrack established) pass, as with ufw. Otherwise the arguments are
# campaign5.sh's: lab bench -> Hetzner listener (rw-easy-to-ufw,
# rw-symmetric-to-ufw) and Hetzner bench -> lab listener (rw-ufw-to-easy,
# rw-ufw-to-symmetric), through the real relay with a temporary directory on
# the relay VM. Writes run<r>-<scenario>/ next to this script.
set -euo pipefail
export LC_ALL=C
here=$(cd "$(dirname "$0")" && pwd)
repo=$(git -C "$here" rev-parse --show-toplevel)
BIN=$repo/target/release/golem-p2p
SSH_CONFIG=$HOME/.ssh/p2p/ssh_config
REAL_RELAY=https://213.163.75.157:56000,quic=56001
REAL_CA=$HOME/.ssh/p2p/p2p-relay1.crt
DIRURL=http://213.163.75.157:56050/
PORT=${UFW_EMUL_PORT:-56095}
REPEAT=${UFW_EMUL_REPEAT:-3}
KEYS=$here/.keys
mkdir -p "$KEYS"
lab() { ssh -F "$SSH_CONFIG" -o ServerAliveInterval=15 p2p-natlab "$@"; }
rel() { ssh -F "$SSH_CONFIG" -o ServerAliveInterval=15 p2p-relay1 "$@"; }
say() { echo "ufw-emul: $(date -u +%T) $*" >&2; }

cleanup() {
    set +e
    sudo nft delete table inet gpl_ufw_emul 2>/dev/null
    rel "sudo systemctl stop gpl-rw-directory 2>/dev/null; sudo systemctl reset-failed gpl-rw-directory 2>/dev/null; rm -rf /tmp/gpl-rw"
    jobs -p | xargs -r kill 2>/dev/null
    say "cleaned up (nft table, directory on the relay VM)"
}
trap cleanup EXIT

[[ $(sha256sum "$BIN" | cut -d' ' -f1) == $(lab "sha256sum golem-p2p-lab/bin/golem-p2p | cut -d' ' -f1") ]] ||
    { echo "binary mismatch" >&2; exit 1; }

say "directory on p2p-relay1"
rel "sudo systemctl stop gpl-rw-directory 2>/dev/null; sudo systemctl reset-failed gpl-rw-directory 2>/dev/null; \
    mkdir -p /tmp/gpl-rw && cat > /tmp/gpl-rw/golem-p2p && chmod 755 /tmp/gpl-rw/golem-p2p" <"$BIN"
rel "sudo systemd-run --quiet --unit=gpl-rw-directory -p User=\$(id -un) -p RuntimeMaxSec=3600 \
    /tmp/gpl-rw/golem-p2p directory --listen 0.0.0.0:56050"
for _ in $(seq 50); do
    code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 2 "${DIRURL}v1/records/0x0000000000000000000000000000000000000000" || true)
    [[ $code == 404 ]] && break
    sleep 0.2
done
[[ $code == 404 ]] || { echo "directory not reachable" >&2; exit 1; }

say "lab up campaign-real"
lab "cd golem-p2p-lab && ./lab.sh up campaign-real >/dev/null"

sudo nft -f - <<NFT
table inet gpl_ufw_emul {
    chain input {
        type filter hook input priority filter; policy accept;
        udp dport $PORT ct state new counter drop
    }
}
NFT
sudo nft list table inet gpl_ufw_emul >"$here/nft-before.txt"

for r in $(seq "$REPEAT"); do
    # Lab bench -> listener here.
    for s in "rw-easy-to-ufw real1" "rw-symmetric-to-ufw real2"; do
        read -r id n <<<"$s"
        d=$here/run$r-$id
        mkdir -p "$d"
        [[ -f $KEYS/hetzner-ufw.key ]] || "$BIN" keygen --out "$KEYS/hetzner-ufw.key" >"$KEYS/hetzner-ufw.id"
        say "round $r $id"
        echo "$(date -u +%FT%TZ) $id run$r $(cut -d' ' -f1-4 /proc/loadavg)" >>"$here/load.txt"
        NO_COLOR=1 timeout -s TERM -k 5 300 "$BIN" --json --run-id "ufw-emul-$id-r$r" --node-name hetzner listen \
            --key "$KEYS/hetzner-ufw.key" --relay "$REAL_RELAY" --relay-ca "$REAL_CA" --directory "$DIRURL" \
            --bind "0.0.0.0:$PORT" >"$d/listen.jsonl" 2>"$d/listen.err" </dev/null &
        lpid=$!
        lid=""
        for _ in $(seq 150); do
            lid=$(sed -n 's/^{.*"event":"ready".*"node_id":"\(0x[0-9a-f]*\)".*/\1/p' "$d/listen.jsonl" | head -1)
            [[ -n $lid ]] && break
            sleep 0.1
        done
        [[ -n $lid ]] || { say "listener not ready"; kill -TERM $lpid; continue; }
        lab "cd golem-p2p-lab && ./lab.sh bench $n --to $lid --out ~/gpl-runs/ufw-emul/run$r-$id --run-id ufw-emul-$id-r$r --directory $DIRURL" \
            2>&1 >/dev/null | tail -n 1 | cut -c1-400 >&2 || true
        kill -TERM $lpid 2>/dev/null || true
        wait $lpid 2>/dev/null || true
        lab "tar -C ~/gpl-runs/ufw-emul/run$r-$id -cf - ." | tar -C "$d" -xf -
    done
    # Bench here -> lab listener.
    for s in "rw-ufw-to-easy real1" "rw-ufw-to-symmetric real2"; do
        read -r id n <<<"$s"
        d=$here/run$r-$id
        mkdir -p "$d"
        say "round $r $id"
        echo "$(date -u +%FT%TZ) $id run$r $(cut -d' ' -f1-4 /proc/loadavg)" >>"$here/load.txt"
        tag=ufwemul-$id-r$r
        ready=$(lab "cd golem-p2p-lab && ./lab.sh listen $n --tag $tag --directory $DIRURL -- --json --run-id ufw-emul-$id-r$r --node-name $n") || {
            say "lab listener not ready"; continue; }
        lid=$(sed -n 's/^{.*"event":"ready".*"node_id":"\(0x[0-9a-f]*\)".*/\1/p' <<<"$ready")
        rc=0
        NO_COLOR=1 timeout -s TERM -k 15 300 "$BIN" --json --run-id "ufw-emul-$id-r$r" --node-name hetzner bench \
            --relay "$REAL_RELAY" --relay-ca "$REAL_CA" --directory "$DIRURL" --to "$lid" --bind "0.0.0.0:$PORT" \
            >"$d/bench.jsonl" 2>"$d/bench.err" </dev/null || rc=$?
        echo "$rc" >"$d/bench.exit"
        lab "cd golem-p2p-lab && ./lab.sh stop $tag; cat /run/gpl/logs/$tag.log" >"$d/listen.jsonl" 2>/dev/null || true
        lab "cat /run/gpl/logs/$tag.err; rm -f /run/gpl/logs/$tag.*" >"$d/listen.err" 2>/dev/null || true
        printf '%s\n' "bencher=hetzner" "listener=$n" "run=ufw-emul-$id-r$r" "to=$lid" \
            "bench_args=--relay $REAL_RELAY --directory $DIRURL --to $lid --bind 0.0.0.0:$PORT" |
            python3 "$repo/harness/lab/tools/benchsum.py" meta "$d/meta.json"
        python3 "$repo/harness/lab/tools/benchsum.py" line "$d" | cut -c1-400 >&2
    done
done
sudo nft list table inet gpl_ufw_emul >"$here/nft-after.txt"
lab "rm -rf ~/gpl-runs/ufw-emul"
say "done"
