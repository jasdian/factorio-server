let
  rustOverlay = builtins.fetchTarball {
    url = "https://github.com/oxalica/rust-overlay/archive/0b3a5ad260479f2c9bdadf3ba5b2a4be359cfcdd.tar.gz";
    sha256 = "1yhnds7765q6r6ik7gr4prrk97ahbfhpfjr1q1z5jr4hrlk0synf";
  };
  pkgs = import <nixpkgs> { overlays = [ (import rustOverlay) ]; };
  muslPkgs = pkgs.pkgsStatic;

  claudeFHS = pkgs.buildFHSEnv {
    name = "claude-fhs";
    targetPkgs = p: with p; [
      stdenv.cc.cc.lib
      zlib
      glib
      nss
      nspr
      dbus
      atk
      cups
      libdrm
      gtk3
      pango
      cairo
      xorg.libX11
      xorg.libXcomposite
      xorg.libXdamage
      xorg.libXext
      xorg.libXfixes
      xorg.libXrandr
      xorg.libxcb
      expat
      alsa-lib
      at-spi2-atk
      at-spi2-core
      libxkbcommon
      mesa
    ];
    runScript = "";
  };
in

pkgs.mkShell {
  nativeBuildInputs = with pkgs; [
    pkg-config
    stdenv.cc
  ];

  buildInputs = with pkgs; [
    openssl
    fontconfig
    muslPkgs.openssl
  ];

  packages = with pkgs; [
    (rust-bin.stable.latest.default.override {
      extensions = [ "llvm-tools-preview" ];
      targets = [ "x86_64-unknown-linux-musl" ];
    })
    rust-bin.stable.latest.rust-analyzer
    rust-bin.stable.latest.rust-src
    cargo-info
    cargo-modules

    muslPkgs.stdenv.cc

    curl
    git
    ripgrep
    claudeFHS

    # Ralph dependencies
    jq      # Required for JSON parsing
    tmux    # Optional: monitoring dashboard (--monitor)
  ];

  RUST_SRC_PATH   = "${pkgs.rust-bin.stable.latest.rust-src}/lib/rustlib/src/rust/library";
  PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig:${pkgs.fontconfig.dev}/lib/pkgconfig";

  CC_x86_64_unknown_linux_musl = "${muslPkgs.stdenv.cc}/bin/x86_64-unknown-linux-musl-gcc";
  AR_x86_64_unknown_linux_musl = "${muslPkgs.stdenv.cc}/bin/x86_64-unknown-linux-musl-ar";
  CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER = "${muslPkgs.stdenv.cc}/bin/x86_64-unknown-linux-musl-gcc";

  OPENSSL_STATIC = "1";
  OPENSSL_DIR = "${muslPkgs.openssl.dev}";
  OPENSSL_LIB_DIR = "${muslPkgs.openssl.out}/lib";
  OPENSSL_INCLUDE_DIR = "${muslPkgs.openssl.dev}/include";
  OPENSSL_LIB_DIR_x86_64_unknown_linux_musl = "${muslPkgs.openssl.out}/lib";
  OPENSSL_INCLUDE_DIR_x86_64_unknown_linux_musl = "${muslPkgs.openssl.dev}/include";

shellHook = ''
  export PATH="$HOME/.local/bin:$PATH"
  RALPH_INSTALL_DIR="$HOME/.local/share/ralph-claude-code"
  CLAUDE_VERSION_CACHE="$HOME/.cache/claude-version"

  # Check if Claude needs update (daily check)
  _claude_check_update() {
    local claude_bin="$HOME/.local/bin/claude"
    local cache_file="$CLAUDE_VERSION_CACHE"
    local check_interval=86400  # 24 hours in seconds

    mkdir -p "$(dirname "$cache_file")"

    # Skip if checked recently
    if [[ -f "$cache_file" ]]; then
      local last_check=$(stat -c %Y "$cache_file" 2>/dev/null || echo 0)
      local now=$(date +%s)
      if (( now - last_check < check_interval )); then
        return 0
      fi
    fi

    # Get current version
    local current_version=""
    if [[ -x "$claude_bin" ]]; then
      current_version=$(claude-fhs "$claude_bin" --version 2>/dev/null | head -1 || echo "")
    fi

    # Fetch latest version from install script
    echo "Checking for Claude updates..."
    local install_script=$(curl -fsSL https://claude.ai/install.sh 2>/dev/null)
    local latest_version=$(echo "$install_script" | grep -oP 'VERSION[=:]\s*["\x27]?\K[0-9]+\.[0-9]+\.[0-9]+' | head -1 || echo "")

    # If we can't determine latest, try npm registry as fallback
    if [[ -z "$latest_version" ]]; then
      latest_version=$(curl -fsSL https://registry.npmjs.org/@anthropic-ai/claude-code/latest 2>/dev/null | grep -oP '"version":\s*"\K[^"]+' || echo "")
    fi

    # Update cache with current check time and versions
    echo "checked=$(date +%s)" > "$cache_file"
    echo "current=$current_version" >> "$cache_file"
    echo "latest=$latest_version" >> "$cache_file"

    # Compare versions (simple string compare, works for semver)
    if [[ -n "$latest_version" && -n "$current_version" && "$current_version" != "$latest_version" ]]; then
      echo "Claude update available: $current_version -> $latest_version"
      echo "Run 'claude-update' to install the latest version."
      return 1
    elif [[ -n "$latest_version" && -z "$current_version" ]]; then
      return 1  # Not installed
    fi

    return 0
  }

  # Force update Claude to latest
  claude-update() {
    echo "Updating Claude to latest version..."
    claude-fhs bash -c 'curl -fsSL https://claude.ai/install.sh | bash'
    rm -f "$CLAUDE_VERSION_CACHE"  # Clear cache to force re-check
    local claude_bin="$HOME/.local/bin/claude"
    if [[ -x "$claude_bin" ]]; then
      local new_version=$(claude-fhs "$claude_bin" --version 2>/dev/null | head -1)
      echo "Claude updated to: $new_version"
    fi
  }
  export -f claude-update

  # Claude wrapper with auto-update check
  claude() {
    local claude_bin="$HOME/.local/bin/claude"

    # Check for updates (non-blocking, just notifies)
    _claude_check_update

    if [[ -x "$claude_bin" ]]; then
      claude-fhs "$claude_bin" "$@"
    else
      echo "Claude not found. Installing inside FHS environment..."
      claude-fhs bash -c 'curl -fsSL https://claude.ai/install.sh | bash'
      if [[ -x "$claude_bin" ]]; then
        claude-fhs "$claude_bin" "$@"
      else
        echo "Installation failed."
        return 1
      fi
    fi
  }
  export -f claude

  # Install Ralph if not present
  if [[ ! -d "$RALPH_INSTALL_DIR" ]]; then
    echo "Installing Ralph..."
    git clone https://github.com/frankbria/ralph-claude-code.git "$RALPH_INSTALL_DIR"
    chmod +x "$RALPH_INSTALL_DIR"/*.sh
  fi

  # Ensure ~/.ralph/templates symlink exists (setup.sh expects it there)
  if [[ -d "$RALPH_INSTALL_DIR/templates" && ! -e "$HOME/.ralph/templates" ]]; then
    mkdir -p "$HOME/.ralph"
    ln -sf "$RALPH_INSTALL_DIR/templates" "$HOME/.ralph/templates"
  fi

  # Ralph wrapper - runs from CURRENT directory (where .ralph/ exists)
  # Uses FHS environment so claude CLI works
  ralph() {
    if [[ ! -d ".ralph" ]]; then
      echo "Error: .ralph/ directory not found in current directory."
      echo "Run 'ralph-setup' first or cd to a Ralph-enabled project."
      return 1
    fi

    # Handle --monitor mode specially: create tmux session where panes
    # re-enter nix-shell to get proper environment (claude wrapper, FHS)
    if [[ "$1" == "--monitor" || "$1" == "-m" ]]; then
      if ! command -v tmux &> /dev/null; then
        echo "Error: tmux is not installed. Install tmux or run without --monitor flag."
        return 1
      fi

      local session_name="ralph-$(date +%s)"

      mkdir -p .ralph/logs
      echo "[$(date '+%Y-%m-%d %H:%M:%S')] [INFO] Setting up tmux session: $session_name" >> .ralph/logs/ralph.log

      # Create tmux session
      tmux new-session -d -s "$session_name" -c "$(pwd)"
      tmux split-window -h -t "$session_name" -c "$(pwd)"

      # Right pane: run monitor with bash explicitly (doesn't need claude/nix-shell)
      tmux send-keys -t "$session_name:0.1" "bash $RALPH_INSTALL_DIR/ralph_monitor.sh" Enter

      # Left pane: re-enter nix-shell and run ralph (without --monitor to avoid recursion)
      # Shift out --monitor and pass remaining args
      shift
      if [[ $# -gt 0 ]]; then
        tmux send-keys -t "$session_name:0.0" "nix-shell --run 'ralph $*'" Enter
      else
        tmux send-keys -t "$session_name:0.0" "nix-shell --run ralph" Enter
      fi

      tmux select-pane -t "$session_name:0.0"
      tmux rename-window -t "$session_name:0" "Ralph: Loop | Monitor"

      echo "[$(date '+%Y-%m-%d %H:%M:%S')] [SUCCESS] Tmux session created. Attaching to session..." >> .ralph/logs/ralph.log
      echo "[$(date '+%Y-%m-%d %H:%M:%S')] [INFO] Use Ctrl+B then D to detach from session" >> .ralph/logs/ralph.log
      echo "[$(date '+%Y-%m-%d %H:%M:%S')] [INFO] Use 'tmux attach -t $session_name' to reattach" >> .ralph/logs/ralph.log

      tmux attach-session -t "$session_name"
    else
      claude-fhs bash -c "'$RALPH_INSTALL_DIR/ralph_loop.sh' \"\$@\"" -- "$@"
    fi
  }
  export -f ralph

  # ralph-setup: Create NEW project with Ralph (runs setup.sh)
  ralph-setup() {
    local project_name="''${1:-.}"
    claude-fhs bash -c "cd '$RALPH_INSTALL_DIR' && ./setup.sh '$project_name'"
  }
  export -f ralph-setup

  # ralph-init: Initialize Ralph in CURRENT directory (copies all templates)
  ralph-init() {
    local templates="$RALPH_INSTALL_DIR/templates"
    if [[ ! -d "$templates" ]]; then
      echo "Error: Templates not found at $templates"
      return 1
    fi
    if [[ -d ".ralph" ]]; then
      echo "Warning: .ralph/ already exists. Backing up to .ralph.bak/"
      rm -rf .ralph.bak
      mv .ralph .ralph.bak
    fi
    echo "Initializing Ralph in current directory..."
    mkdir -p .ralph/{specs,logs,docs/generated}
    cp "$templates/PROMPT.md" .ralph/
    cp "$templates/fix_plan.md" .ralph/@fix_plan.md
    cp "$templates/AGENT.md" .ralph/@AGENT.md
    cp -r "$templates/memory" .ralph/
    cp -r "$templates/skills" .ralph/
    cp -r "$templates/orchestration" .ralph/
    cp -r "$templates/specs"/* .ralph/specs/ 2>/dev/null || true
    echo "✅ Ralph initialized. Edit .ralph/specs/ and .ralph/@fix_plan.md"
  }
  export -f ralph-init

  # ralph-import: Import requirements into Ralph project
  ralph-import() {
    local requirements_file="$1"
    local project_name="''${2:-.}"
    if [[ -z "$requirements_file" ]]; then
      echo "Usage: ralph-import <requirements.md> [project-name]"
      return 1
    fi
    claude-fhs bash -c "cd '$RALPH_INSTALL_DIR' && ./ralph_import.sh '$requirements_file' '$project_name'"
  }
  export -f ralph-import

  # ralph-update: Update Ralph to latest version
  ralph-update() {
    echo "Updating Ralph..."
    (cd "$RALPH_INSTALL_DIR" && git pull)
  }
  export -f ralph-update

  echo "Rust dev shell"
  echo "  glibc:  cargo build --release"
  echo "  MUSL:   cargo build --release --no-default-features --target x86_64-unknown-linux-musl"
  echo "  Claude: claude (wrapped with claude-fhs)"
  echo ""
  echo "Ralph (autonomous Claude Code):"
  echo "  ralph-init         Initialize Ralph in CURRENT directory"
  echo "  ralph-setup NAME   Create NEW project with Ralph"
  echo "  ralph-import FILE  Import requirements into project"
  echo "  ralph              Run autonomous development loop"
  echo "  ralph --monitor    Run with tmux dashboard"
  echo "  ralph-update       Update Ralph to latest"
'';
}