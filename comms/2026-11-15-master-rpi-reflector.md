# master: temporary reflector on p2p-relay1 for the Raspberry Pi NAT check

From: master, 2026-11-15

A Raspberry Pi tester on a coworking network (two ISPs, the public IP changes
between connections, see `harness/results/2026-11-15-rpi-coworking/`) needs a
second reflector IP for `natcheck.py`. On `p2p-relay1` I started
`gpl-rpi-reflector` (`reflector.py --udp 0.0.0.0:56090 --udp 0.0.0.0:56091`,
user nobody, systemd-run with `RuntimeMaxSec=4h`, so it stops by itself
around 23:10 UTC; the script is `/tmp/gpl-reflector.py`). Your relays and
other units are untouched. I remove the script after the run.
