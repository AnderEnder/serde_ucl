# Benchmark corpus

Real UCL configurations for the benchmarks (clean-room work item C14). The spec team copies them
here as single files at a pinned commit, unchanged. They are an allowed input for the
implementation team as documents; the repositories they come from are not
(`docs/clean-room/PROTOCOL.md`).

## rspamd

From [rspamd](https://rspamd.com), directory `conf/` at commit
`5fc47041c8315eb2d6f20bc36e7d006448c47695`, under the Apache License 2.0: `rspamd/LICENSE.md` is
rspamd's `LICENSE.md` at that commit. rspamd has no NOTICE file at that commit. Each file was
fetched as
`https://raw.githubusercontent.com/rspamd/rspamd/5fc47041c8315eb2d6f20bc36e7d006448c47695/conf/<path>`.

The documents:

| Document | Bytes | What it needs |
| --- | ---: | --- |
| `rspamd/groups.conf`, with the 14 files of `rspamd/scores.d/` it includes | 55,215 | the variable `CONFDIR` set to the directory of `groups.conf`, and a file loader that reads `scores.d/` there |
| `rspamd/composites.conf` | 9,155 | nothing |
| `rspamd/scores.d/rbl_group.conf` | 13,404 | nothing |

- `groups.conf` holds 21 `group` sections; 14 of them include a file of `scores.d/` with a plain
  `.include "$CONFDIR/scores.d/<name>.conf"`, which must be found. Every other include in
  these documents is `.include(try=true; ...)` on `$LOCAL_CONFDIR`, which may be left unset:
  the includes then find nothing, and the documents parse as shown. The parse of
  `groups.conf` gives 21 groups with 253 symbols.
- Each file of `scores.d/` is also a document of its own, with no includes: nested sections,
  quoted strings, floats and integers. `rbl_group.conf` is the largest.
- `composites.conf`: 9 KB of sections with string expressions, arrays and numbers.

Checked with the differential fuzzer, whose `--check` parses a file with both libucl and the
crate (it always adds `string-input`); every document agrees:

    target/fuzz/release/ucl-differential --check benches/corpus/rspamd/groups.conf \
        --dir benches/corpus/rspamd --flag var:CONFDIR=.
    target/fuzz/release/ucl-differential --check benches/corpus/rspamd/<file> --dir benches/corpus/rspamd

Left out: `conf/modules.d/rbl.conf` and `conf/modules.d/multimap.conf`, which rspamd runs
through its Jinja templating (`{= ... =}`) before parsing, so that neither libucl nor the crate
parses them as they are; `conf/modules.d/antivirus.conf`, almost only comments.

| File | Source | SHA-256 |
| --- | --- | --- |
| `rspamd/groups.conf` | `conf/groups.conf` | `75147e3493300b27dc95ca41e5160297c84e2335c9b4f311e876d9f40b947425` |
| `rspamd/composites.conf` | `conf/composites.conf` | `3773cf9d42ee506b823a2d7867fdc378136414c1c1104c4853c98152d54be067` |
| `rspamd/scores.d/content_group.conf` | `conf/scores.d/content_group.conf` | `fe8ca00bb2948bdaaacb4b91356834c0e3cb869ec02b07ab0af017e1692eaa80` |
| `rspamd/scores.d/fuzzy_group.conf` | `conf/scores.d/fuzzy_group.conf` | `dd94d37a612d7bfbb073e0f36c9ca8a5e609011636930cf52aadaace3311637f` |
| `rspamd/scores.d/headers_group.conf` | `conf/scores.d/headers_group.conf` | `83f2236714d2fd16099bd53b8f3c2641b884f64a40d1b87cfc23f9681c7f1771` |
| `rspamd/scores.d/hfilter_group.conf` | `conf/scores.d/hfilter_group.conf` | `adb480e6ecb629f88df0d6ca7bac30cff1c4f27c6f58615489472b95cc50c20c` |
| `rspamd/scores.d/mime_types_group.conf` | `conf/scores.d/mime_types_group.conf` | `1225dc23658271c139538435f692fc76e2db756da0f4412817bf9319bb180d6a` |
| `rspamd/scores.d/mua_group.conf` | `conf/scores.d/mua_group.conf` | `9a16f439be6d8bfc0058133d36b88fccee7440fbc72bf1c13d9fc94f61bd6f70` |
| `rspamd/scores.d/phishing_group.conf` | `conf/scores.d/phishing_group.conf` | `5204a60302bfd093dcdf1c590afa848697133fdaf4b4dc49f4c595cf5b3954e3` |
| `rspamd/scores.d/policies_group.conf` | `conf/scores.d/policies_group.conf` | `2bf8cf32451493e96acfea1232342c37b3042e29389ac6f7269b9c57cac4f53d` |
| `rspamd/scores.d/rbl_group.conf` | `conf/scores.d/rbl_group.conf` | `edde5d5c5877d7e04340cc98b9f30e1c4a264ef99a18dcd99e7e4aeff8c99dc4` |
| `rspamd/scores.d/statistics_group.conf` | `conf/scores.d/statistics_group.conf` | `548a49d660dec31c37c067fcdd6e2d093822be14a62b98b1495a86ce979111ba` |
| `rspamd/scores.d/subject_group.conf` | `conf/scores.d/subject_group.conf` | `5d24be19d74e4bbc54a8aab3229fb28255e6680e6f0dfdc5359b5bbc68a65014` |
| `rspamd/scores.d/surbl_group.conf` | `conf/scores.d/surbl_group.conf` | `eaf1c0cb6bbb64aec4ea27cce7b686927ebc4a4651d3f3a224c1a12ba7c7534b` |
| `rspamd/scores.d/url_suspect_group.conf` | `conf/scores.d/url_suspect_group.conf` | `9019af0eea8636b99f65f4a3a87238248866061848cfe1c7ce8facf3a496a200` |
| `rspamd/scores.d/whitelist_group.conf` | `conf/scores.d/whitelist_group.conf` | `776139f22257ccb4d2ee191f99cda1e5fe351ee94401c2a763b2e2e505fabf9d` |
| `rspamd/LICENSE.md` | `LICENSE.md` | `a2bbe5f2ccbdfbbfee378f32da4684e240fa19c8041f50dd135c77a39dd13b7a` |
