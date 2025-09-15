# binup

binup tries to become the last tool you'll manually install from GitHub releases. It focuses on single binary apps and allows you to automatically install and upgrade them once you specify rules of how to find each app in release files.

Here is how it works. You run `binup install --project KonishchevDmitry/binup` and it finds and installs the binary from [project releases](https://github.com/KonishchevDmitry/binup/releases).

All installed tools are registered in `~/.config/binup/config.yaml` which you may edit manually. binup uses [nondestructive](https://github.com/udoprog/nondestructive/) for config editing, so it tries to preserve the configuration file structure and comments.

When tool is registered in the configuration file, you may install/reinstall/upgrade it by name: `binup install|upgrade $name`. If tool name is not specified, binup installs/upgrades all registered tools.

Except for the configuration file, binup is fully stateless: it doesn't save any information about installed binaries. Instead, is always checks the actual state of the apps: if binary is missing, it installs it. When the binary is already installed, it runs it with `--version` argument and tries to parse its actual version to compare with the latest release. If it fails to determine the version (the tool might not have `--version` flag), binup relies on binary file modification time, always setting it to update time of the downloaded release archive.

## Available commands

### binup
```
Automated app installation from GitHub releases

Usage: binup [OPTIONS] <COMMAND>

Commands:
  list       List all configured tools [aliases: l]
  install    Install all or only specified tools [aliases: i]
  upgrade    Upgrade all or only specified tools [aliases: u]
  uninstall  Uninstall the specified tools [aliases: remove, r]

Options:
  -c, --config <PATH>  Configuration file path [default: ~/.config/binup/config.yaml]
  -v, --verbose...     Set verbosity level
  -h, --help           Print help
  -V, --version        Print version
```

### binup list
```
List all configured tools

Usage: binup list [OPTIONS]

Options:
  -u, --prerelease  Don't filter out prerelease versions
  -f, --full        Show full information including changelog URL
  -h, --help        Print help
```

### binup install
```
When no arguments are specified, installs all the tools from the configuration file which
aren't installed yet. When tool name(s) is specified, installs this specific tool(s).

When --project is specified, adds a new tool to the configuration file and installs it.

Usage: binup install [OPTIONS] [NAME]...

Arguments:
  [NAME]...  Tool name

Options:
  -f, --force                      Force installation even if tool is already installed
  -p, --project <NAME>             GitHub project to get the release from
  -u, --prerelease                 Allow installation of prerelease version
  -c, --changelog <URL>            Project changelog URL
  -r, --release-matcher <PATTERN>  Release archive pattern
  -b, --binary-matcher <PATTERN>   Binary path to look for inside the release archive
  -v, --version-source <SOURCE>    Method which is used to determine current binary version [default: flag] [possible values: flag, command]
  -d, --path <PATH>                Path where to install this specific tool to
  -s, --post <COMMAND>             Post-install command
  -h, --help                       Print help (see more with '--help')
```

### binup upgrade
```
Upgrade all or only specified tools

Usage: binup upgrade [NAME]...

Arguments:
  [NAME]...  Tool name

Options:
  -u, --prerelease  Allow upgrade to prerelease version
  -h, --help        Print help
```

### binup uninstall
```
Arguments:
  <NAME>...  Tool name

Options:
  -h, --help  Print help
```

## Available configuration options

Here is an example config with all available configuration options:
```yaml
# Path where to install the binaries (the default is ~/.local/bin)
path: /usr/local/bin

tools:
  # Binary name
  prometheus:
    # GitHub project name
    project: prometheus/prometheus

    # Allow installation of prerelease versions (default: false)
    #
    # If project has only prerelease versions, they aren't considered as prerelease and will be installed without this option.
    prerelease: true

    # Changelog URL (will be printed on app upgrade)
    changelog: https://github.com/prometheus/prometheus/blob/main/CHANGELOG.md

    # Release archive pattern:
    # * By default shell-like glob matching is used (https://docs.rs/globset/latest/globset/#syntax)
    # * Pattern started with '~' is treated as regular expression (https://docs.rs/regex/latest/regex/#syntax)
    #
    # If it's not specified, the archive will be chosen automatically according to target platform.
    release_matcher: prometheus-*.linux-amd64.tar.gz

    # Binary path to look for inside the release archive. If it's not specified, the tool will try to find it automatically.
    binary_matcher: "*/prometheus"

    # Method which is used to determine current binary version:
    # * flag: `binary --version`
    # * command: `binary version`
    #
    # The default is flag.
    version_source: flag

    # Path where to install this specific tool to
    path: ~/bin

    # Post-install command
    post: systemctl restart prometheus

# If you have a lot of tools, you may hit GitHub API rate limits for anonymous requests at some moment.
# So it's recommended to obtain GitHub token (https://github.com/settings/tokens) and specify it here.
# No permissions are required for the token – it's needed just to make API requests non-anonymous.
github:
  token: $token
```

binup edits the configuration file only in the following cases:
1. When `--project` is specified in the `install` command and the specified parameters doesn't match already registered ones;
2. In the `uninstall` command.

If you don't feel comfortable when some app automatically edit your configs, you can register all tools manually and run `binup install|upgrade $name` – when `--project` is not specified, the tool never touches the config.

## Development
Tested on on Ubuntu 25.04.
Install compiler
```
sudo apt-get install build-essential
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
sudo apt install git curl -y
$ cargo --version
cargo 1.89.0 (c24e10642 2025-06-23)
```
Build binup
```
git clone https://github.com/KonishchevDmitry/binup/
cd binup/
cargo build --release
```
Run unit tests and integration tests
```
cargo test
```
Install
```
cargo install --path "/usr/local/bin/"
```
Check
```
$ target/debug/binup --version
binup 1.7.0
```