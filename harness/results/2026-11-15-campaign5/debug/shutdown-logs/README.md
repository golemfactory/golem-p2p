# Shutdown log lines with the iroh fork (local repro)

Campaign 5's `.err` files often end with

    ERROR iroh::socket::transports::relay: relay_recv_channel closed
    WARN  iroh::socket::transports: All transports failed to receive. QUIC endpoint will be shutdown.
    ERROR noq::endpoint: I/O error: All transports failed to receive

always during `Endpoint::close` at the end of a process (campaign 4 had none).
`repro.sh <golem-p2p binary> <label> 10` (run on master, 2026-11-15 12:16-12:25
UTC) starts a local iroh-relay 1.5.0 (fork build, QAD on) and a directory on
127.0.0.1, then runs 10 local checks (`bench` without `--to`) and 10
listen/bench pairs (1 MB transfers, no concurrency phase), and counts the
processes whose stderr has `relay_recv_channel closed`:

| golem-p2p (crates of 31c954c) built with | local checks | benches (pairs) | listeners | total |
|---|---|---|---|---|
| iroh 1.4.1 (crates.io; Cargo files of 1634cc3) | 0 / 10 | 0 / 10 | 0 / 10 | 0 / 30 |
| iroh upstream main 2c12bb58 (1.5.0 without the fork's commit; local path patch) | 0 / 10 | 0 / 10 | 0 / 10 | 0 / 30 |
| iroh fork 4ad24ac2 (the campaign's build) | 4 / 10 | 0 / 10 | 3 / 10 | 7 / 30 |

So the lines come from the fork's relay-recovery commit (4ad24ac2). At
shutdown its relay actors skip the graceful WebSocket close (cancelled by the
stop token) and drop the datagram channel while the QUIC endpoint still polls
the relay transport, whose `poll_recv` logs the closed channel at ERROR level
and reports an error, so noq gives up receiving. Nothing is lost (the
endpoint is closing anyway). `example-fork-check.err` is one such run. The
builds and their output were temporary and are not kept.
