# Carbon

The local hub for everything in `~/dev`: a macOS app, a command, and a dashboard for the dev servers on your
machine. It is always on **http://localhost:3000**, and it decides what runs on the ports next to it:

| Port | What runs there |
| --- | --- |
| 3000 | Carbon |
| 3001 | Website: a project's site (`web`, `website`, `site`, or the project itself) |
| 3002 | Admin: a folder named `admin` |
| 3003 | Pro: a folder named `coach` or `pro` |
| 3004–3019 | Everything else, one port each |

Running an app puts it on its port and stops whatever was there, so the same kind of thing is always at the same
address.

## Three ways in

```bash
mise run app        # build Carbon.app into dist/ and open it
mise run install    # install the `carbon` command into ~/.cargo/bin
mise run start      # or just run the hub in this terminal
```

- **The app** is the hub: it runs the engine inside itself and shows the dashboard in a window. Closing the window
  leaves Carbon in the menu bar with every server still running; quitting from the menu bar stops Carbon and the
  servers it started.
- **The command** talks to whichever hub is running (the app, or `carbon serve`):

  ```
  carbon ls               what is on each port, and what else is running
  carbon agents           the servers coding agents have started, by session
  carbon run <app>        start an app on its port (a project's name starts all of it)
  carbon stop <app|port>  stop what is on a port
  carbon open <app|port>  open it in the browser
  carbon logs <port> [-f] what a server Carbon started has printed
  carbon new <group>/<name> [start|spa|bun]
  ```

- **The dashboard** is the same page in any browser at `localhost:3000`, with a ⌘K palette for run, stop and open.

## Servers started by agents

Coding agents start dev servers for you to look at, then leave them running. Carbon finds them: every server a
coding agent started is listed under the session that started it, with the agent's mark, its port, and a Stop
button. Only servers are shown: a build or a test run an agent kicked off is not Carbon's business. A port tile
carries the agent's mark when an agent started what is on it. Servers you started by hand are under "Elsewhere".

Each agent is a **harness adapter** in `src/harness.rs`: how to recognise its process and, where it keeps records,
which session a process is and what that session is called. Claude Code's adapter reads its session files, so a
session shows its name and whether it is working or waiting. Codex, Gemini CLI and Grok CLI are recognised; their
adapters don't read session records yet. Adding a harness is one adapter and one line in `ALL`.

## What it can run

A folder under `~/dev/<group>/<project>` (or one level inside a project) is runnable when, in this order:

1. its `mise.toml` has a `dev` task: `mise run dev`
2. its `package.json` has a `dev` script: that script. Vite, Next and Astro are told their port by flag
3. its `deno.json` has a `dev` task: `deno task dev`
4. it has a `Procfile` with a `web:` line: that line
5. it is a Rust crate built on a web server: `cargo run`
6. it is a Go program that listens: `go run .`

Anything not told its port by flag gets it in the environment: `PORT`, and `ADDRESS` / `ADDR` as `:port`.

Everything is started through `mise exec`, so each project runs on the tool versions it names. A project that names
none shows a "pin" button, which writes its runtime's version into its own `mise.toml`. A runtime the machine doesn't
have (Deno, say) is fetched by mise the first time. A project whose dependencies were never installed gets them
installed first. If a server stops by itself, its port shows the last thing it printed.

## New projects

"New project" (or `carbon new`) creates `~/dev/<group>/<name>` from a template (TanStack Start, TanStack Router, or
an empty Bun project) and pins Bun in its `mise.toml`.

## New tabs in Helium

`helium/` is a small extension that makes every new tab the dashboard (or a note saying Carbon isn't running).
Install it once: open `helium://extensions`, turn on Developer mode, choose "Load unpacked" and pick this
repository's `helium` folder.

## How it is built

- `src/` is the engine, the server and the command, in Rust: `engine.rs` (finding apps, starting and stopping,
  reading what is listening from the OS), `detect.rs` (how to run each kind of project), `agents.rs` and
  `harness.rs` (who started what), `create.rs` (new projects), `server.rs` (the HTTP API and the page),
  `client.rs` and `main.rs` (the command).
- `desktop/` is the macOS app, in Rust (Tauri): the same engine in a window with a menu-bar icon.
- `ui/` is the page, in TypeScript, because it runs in a browser: Vite, React, TanStack Router, Tailwind, Zustand.
  It is built into the binary, so the command and the app each carry it.
- `brand/` is the logo: `mark.svg` (the mark alone, in the text colour), `icon.svg` and `icon-1024.png` (white on a
  black tile) and `Carbon.icns`.

Carbon only listens on this machine and only answers requests addressed to `localhost:3000`.

The agent marks in `ui/src/logos.tsx` are from the `@lobehub/icons-static-svg` set (MIT); each is its owner's
trademark, used only to say which tool started something.

```bash
mise run check    # cargo test, then types, oxlint and oxfmt for the page
```
