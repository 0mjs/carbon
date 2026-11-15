# Carbon

A desktop app, a command, and a dashboard for the dev servers on your machine. It finds your projects by itself,
is always on **http://localhost:3000**, and decides what runs on the ports next to it:

| Port | What runs there |
| --- | --- |
| 3000 | Carbon |
| 3001 | Website: a project's site (`web`, `website`, `site`, `frontend`, or the project itself) |
| 3002 | Admin: a folder named `admin` or `backoffice` |
| 3003 | Pro: a folder named `pro`, `dashboard`, `console` or `portal` |
| 3004–3019 | Everything else, one port each |

Running an app puts it on its port and stops whatever was there, so the same kind of thing is always at the same
address.

Carbon is early: this is 0.0.2. It is built and used on macOS; the engine and the app are written to be portable,
but Linux and Windows are untried.

## Three ways in

You need [mise](https://mise.jdx.dev) (Carbon starts everything through it, and it supplies Bun for building the
page) and Rust from [rustup](https://rustup.rs), which picks up the version in `rust-toolchain.toml` by itself.

```bash
git clone https://github.com/0mjs/carbon && cd carbon
mise run install-app  # build Carbon.app and put it in /Applications
mise run app          # build Carbon.app into dist/ and open it
mise run install      # install the `carbon` command into ~/.cargo/bin
mise run start        # or just run the hub in this terminal
```

- **The app** is the hub: it runs the engine inside itself and draws its window natively with
  [Rune](https://github.com/anomalyco/rune), with no web view, so the same app can be built for macOS, Windows and
  Linux. The first time it opens it asks where your projects are. After that there are two things to look at,
  chosen from the list on the left: **Running**, every server on the machine with a picture of its page, and
  **a project**, with each of its apps to run, open and stop. Search the list with ⌘K, and pin the projects you
  use most to the top of it. Closing the window leaves Carbon and every server running (click the Dock icon to bring it back);
  quitting stops Carbon and the servers it started.
- **The command** talks to whichever hub is running (the app, or `carbon serve`):

  ```
  carbon ls               what is on each port, and what else is running
  carbon agents           the servers coding agents have started, by session
  carbon run <app>        start an app on its port (a project's name starts all of it)
  carbon stop <app|port>  stop what is on a port
  carbon open <app|port>  open it in the browser
  carbon logs <port> [-f] what a server Carbon started has printed
  carbon new <folder>/<name> [start|spa|bun]
  ```

- **The dashboard** is the same page in any browser at `localhost:3000`, with a ⌘K palette for run, stop and open.

## Servers started by agents

Coding agents start dev servers for you to look at, then leave them running. Carbon finds them: a server an agent
started says so, with the agent's mark, the name of the session that started it, the app that session is running
in, and the model it is using. Only servers are shown: a build or a test run an agent kicked off is not Carbon's
business.

Three things are told apart:

- **The harness** is the program that runs commands for you. Carbon recognises Claude Code, Codex, Gemini CLI,
  Grok CLI, pi, oh-my-pi, OpenCode, Amp, Cursor Agent, Copilot CLI, Droid, Qwen Code, Kilo Code, Cline, Auggie,
  Crush, Goose and Aider, by the program and package names each publishes. Each is one entry in `src/harness.rs`.
- **The host** is the app the harness is running inside: an editor, a terminal, or an app made for running agents
  (T3 Code, Orca, Tern and the like). Nothing is listed for this: Carbon walks up from the session to the nearest
  app and shows that app's own name and icon, so an app needs no support added. It is macOS only for now, and has
  been tried with Tern.
- **The session and its model** come from the harness's own records, where Carbon can read them. Today that is
  Claude Code only: its sessions show their name, whether they are working or waiting, and the model behind the
  latest answer. The other harnesses are recognised without those details.

A harness with a mark in `marks/` shows it; one without shows its first letter.

## Where it looks

Carbon starts from your home folder. Any top-level folder there that holds a project within two levels is a place
code lives (`~/dev`, `~/Projects`, `~/code`…); Documents, Desktop, Downloads, Library and the media folders are left
alone. Inside such a place the layout is read, not assumed: a folder with a `.git`, a `package.json` or similar is a
project, the folders inside it are its parts, and a folder with neither is a group of projects.

The app's first screen shows what was found, every folder and project ticked. Untick what you don't want, or add
a folder Carbon didn't think to look in (your projects needn't be tidy: any folder inside your home folder will
do). "Choose projects" at the bottom of the list brings the screen back.

What you choose there is kept in `~/.config/carbon/config.json`, which you can also write by hand. All of it is
optional:

```json
{
  "roots": ["~/work", "~/Documents/code"],
  "hidden": ["~/work/old-site"],
  "roles": { "~/work/shop/coach": "pro" },
  "browser": "Helium",
  "window": "fullscreen"
}
```

`roots` says exactly where to look, `hidden` leaves projects out, `roles` puts a folder on a particular port (`website`, `admin`, `pro` or
`other`), `browser` chooses what links open in (the system's default without it), and `window` opens the app's
window `fullscreen` or `maximized` (at its own size without it).

## Projects, names, logos and previews

A project is called what it already calls itself: the name in its web manifest, its Expo config or its Xcode
project, else its folder's name. Its logo is the best icon it ships (`apple-touch-icon.png`, `icon.svg`,
`favicon.svg`… in `public/`, `app/`, `assets/` and the like), else its first letter on a colour of its own.
In the app, **Edit** on a project changes either: type a name, choose an image, or drop one on the page. What
you chose is kept in `~/.config/carbon/projects.json`, with logos copied to `~/.config/carbon/logos/`. Pins are
kept there too.

Beside each app are the marks of what it is built with: the tool its dev command runs, its runtime, and the
well-known packages it depends on (React, Tailwind, TypeScript and so on), read from its own manifest. The list is
in `src/stack.rs`.

For each app Carbon shows its branch, when it was last committed to, how many files are uncommitted, and the last
commit's first line. Every port that is serving shows a picture of its page, retaken every twenty seconds by a
Chromium-family browser (Chrome, Chromium, Brave, Edge, Helium or Vivaldi) run without a window.

## What it can run

A project, or a folder one level inside one (two under `apps/`, `packages/`, `services/` or `sites/`), is runnable
when, in this order:

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

"New project" (or `carbon new`) creates a project in one of the folders Carbon found, from a template (TanStack Start, TanStack Router, or
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
- `src/places.rs` finds where code lives and reads the config.
- `src/identity.rs` works out what a project is called and what its logo is, and keeps what was chosen by hand.
- `src/stack.rs` reads what each app is built with.
- `marks/` holds the marks of the harnesses and the tools, shared by the app and the page. Its README says where
  each comes from.
- `desktop/` is the desktop app, in Rust, drawn with Rune: the same engine and server inside a native window.
  `carbon-desktop --snap out.png --scene now` renders the window to a picture without opening one (`--scene project`
  for a project's page, `--theme light` for the light look).
- `ui/` is the page, in TypeScript, because it runs in a browser: Vite, React, TanStack Router, Tailwind, Zustand.
  It is built into the binary, so the command and the app each carry it.
- `brand/` is the logo: `mark.svg` (the mark alone, in the text colour), `icon.svg` (the app icon as a drawing), and
  `icon-1024.png` and `Carbon.icns`, which `brand/build.sh` makes with ImageMagick.

Carbon only listens on this machine and only answers requests addressed to `localhost:3000`.

```bash
mise run check    # cargo test, then types, oxlint and oxfmt for the page
```

## Licence

MIT: see [LICENSE](LICENSE). The marks in `marks/` are their owners' trademarks, used only to say which tool
started or runs something; Carbon is not affiliated with or endorsed by any of them.
