# nodes

Shared notes between the machines that test golem-p2p together. Every machine
(a "node" here) has one file in this folder that says what it is, how to reach
it and what it runs. Other nodes read it with `git pull`.

```
nodes/README.md     this file: the folder's purpose and the push rules
nodes/<node>.md     one file per node, kept up to date by that node
```

`<node>` is the node's short name: the first label of its DNS name if it has
one (`master` for `master.golemp2p.arkiv-global.net`), otherwise its hostname.

## What a node file contains

* **Role:** master, relay, client, lab, ...
* **Reach:** DNS name, public IPv4/IPv6, NAT situation if known.
* **Services:** what runs there (relay, directory, listener) with URLs and
  ports, and whether it is up. Write "planned" for what does not run yet.
* **Machine:** OS, kernel, CPU, RAM, Rust toolchains.
* **Updated:** the date of the last change (UTC, `YYYY-MM-DD`).

Results of test runs do not go here; they go to `harness/results/`.

## Push rules

The repository is in development: push straight to `main`, no branches or pull
requests needed for this folder.

1. **Author.** Commit as `scx1332 <sieciech.czajka@golem.network>` only, set
   in the clone:

       git config user.name scx1332
       git config user.email sieciech.czajka@golem.network

   No other author, no `Co-Authored-By` or other attribution trailers.
2. **Your own file only.** A node changes `nodes/<node>.md` of itself. Change
   another node's file or this README only when asked to.
3. **Pull before you push.** `git pull --rebase origin main`, then push. If the
   push is rejected, pull with rebase again; never `git push --force` to
   `main`.
4. **No secrets.** No tokens, keys (node, iroh endpoint, TLS), passwords or
   `.env` contents. Public addresses, DNS names, relay URLs and node IDs are
   fine.
5. **Small commits, one subject.** Message `nodes/<node>: <what changed>`, for
   example `nodes/master: relay up on 443`.
6. **Code changes** follow the usual checks before pushing:
   `cargo fmt --all --check`, `cargo +1.96.0 clippy --workspace --all-targets
   --all-features -- -D warnings` (the toolchain CI pins) and
   `cargo test --workspace --all-features`.
