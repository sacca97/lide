# Remote workspaces over SSH

Status: **remote file editing, one file at a time.** SSH terminals work, the
agent serves files, and `SSH: open remote file` opens a remote text or
Markdown document through the agent; saving writes it back. A remote *tree*,
search, Git, LSP, terminals on the host and the remote PDF/image/table/
notebook viewers are still to come.

## What exists

- **SSH terminal.** Palette → **SSH: connect** lists `Host` aliases from
  `~/.ssh/config` and runs `ssh host` in an integrated terminal.
- **`lide-agent`.** A small stdio program that serves a directory tree over
  `Content-Length` framed JSON-RPC. It is the same framing as the LSP client
  (`lide-rpc`), so it rides an ordinary SSH command:
  `ssh host lide-agent --stdio --root /path/to/project`.
- **`lide-remote`.** The local client: `Remote::spawn_local` for tests and
  local use, `Remote::ssh` for a host, with typed `hello`, `list`, `stat`,
  `read`, `write`, `rename`, `remove` and `mkdir`.
- **Remote documents.** `AgentStore` implements `lide_resource::Store`; the
  editor's `open_with`/`save` go through it, so remote files keep their
  encoding and line endings exactly as local ones do. Palette → **SSH: open
  remote file**. The connection and the read run on a thread; the save is
  still synchronous and will move off the UI thread with the rest of the
  remote file operations.

The agent is on `PATH` only after it is installed on the remote:

```sh
make agent
scp "$(make -s where-agent)" host:~/.local/bin/lide-agent
```

`make install` installs both `lide` and `lide-agent` locally.

## Protocol 1

One JSON-RPC request per framed message:

```json
{"jsonrpc":"2.0","id":1,"method":"read","params":{"path":"src/main.rs","offset":0,"length":8388608}}
```

Methods:

| Method | Parameters | Result |
| --- | --- | --- |
| `initialize` | — | `{protocol, version, root}` |
| `list` | `path` (`""` = root) | `{entries: [{name, path, is_dir, size, modified}]}` |
| `stat` | `path` | `{entry}` |
| `read` | `path`, `offset`, `length` | `{data (base64), next, size, eof}` |
| `write` | `path`, `data (base64)` | `{written}` |
| `rename` | `from`, `to` | `{renamed}` |
| `remove` | `path`, `recursive` | `{removed}` |
| `mkdir` | `path` | `{created}` |

`read` is chunked at 8 MiB so a large PDF or image can be fetched without
exceeding the 64 MiB message limit. `write` is limited to 32 MiB for now;
saving larger files needs a chunked write before it can be supported.

## Trust model

- No port is opened and no cryptography is implemented: transport is the
  system `ssh`, using keys or an agent.
- `BatchMode=yes` is set, so a missing key fails instead of waiting for a
  password prompt that the window cannot show.
- Paths are relative to `--root`. Absolute paths and `..` components are
  rejected; existing paths are canonicalised and must still start with the
  root, which also refuses symlinks that point outside it. The root itself
  cannot be removed or renamed.
- The agent runs with the remote user's privileges. It is not a sandbox; do
  not run it for an untrusted account.

## Remaining integration

1. **Resource model, remaining.** `Editor` now holds a `Location` and a
   `Store`; `Workspace` still derives local paths from it and uses `PathBuf`
   for its tree, watcher, recents, LSP, Git and viewers. Those need the same
   treatment.
2. **Tree and watcher.** List through the agent, poll or add a watch method;
   `notify` is local-only.
3. **Search, Git, LSP and terminals on the host.** Add agent methods for
   `rg --json`, `git`, server stdio and a PTY; route the existing clients
   through them.
4. **Remote PDF.** `PdfView` needs to render from bytes fetched through the
   agent rather than `load_pdf_from_file` on a local path; the chunked `read`
   already returns whole files.
5. **Agent install/version handshake.** Copy the matching agent on connect
   (checksum, version check) instead of asking the user to `scp` it.
6. **Port forwarding** and reconnect, then a host manager.
