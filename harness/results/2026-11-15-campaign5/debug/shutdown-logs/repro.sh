#!/usr/bin/env bash
# repro.sh <golem-p2p binary> <label> [N]: local relay + directory; N x bench local check
# and N x listen/bench pairs; counts "relay_recv_channel closed" in stderr.
set -u
R=$(cd "$(dirname "$0")" && pwd); B=$1; L=$2; N=${3:-10}
RELAY_BIN=${RELAY_BIN:-$R/../relay150/iroh-relay}
out=$R/out-$L; rm -rf "$out"; mkdir -p "$out"
$RELAY_BIN --config-path $R/relay.toml >"$out/relay.log" 2>&1 & RP=$!
$B directory --listen 127.0.0.1:56086 >"$out/dir.log" 2>&1 & DP=$!
sleep 1
REL="https://127.0.0.1:56084,quic=56085"
C=(--relay "$REL" --relay-ca $R/relay.crt --directory http://127.0.0.1:56086/)
for i in $(seq $N); do
  NO_COLOR=1 timeout 60 $B --json bench "${C[@]}" >"$out/check$i.jsonl" 2>"$out/check$i.err"
done
[[ -f $out/l.key ]] || $B keygen --out "$out/l.key" >"$out/l.id"
for i in $(seq $N); do
  NO_COLOR=1 $B --json listen --key "$out/l.key" "${C[@]}" >"$out/listen$i.jsonl" 2>"$out/listen$i.err" & LP=$!
  for _ in $(seq 100); do grep -q '"ready"' "$out/listen$i.jsonl" && break; sleep 0.1; done
  NO_COLOR=1 timeout 120 $B --json bench "${C[@]}" --to $(cat "$out/l.id") --transfer-size 1M --skip concurrency >"$out/pair$i.jsonl" 2>"$out/pair$i.err"
  kill -TERM $LP; wait $LP 2>/dev/null
done
kill $RP $DP; wait 2>/dev/null
for k in check pair listen; do
  n=0; t=0; for f in $out/$k*.err; do t=$((t+1)); grep -q "relay_recv_channel closed" $f && n=$((n+1)); done
  echo "$L $k: $n/$t with relay_recv_channel closed"
done
