# binup

binup tries to become the last tool you'll manually install from GitHub releases. It focuses on single binary apps and allows you to automatically install and upgrade them once you specify rules of how to find each app in release files.

Here is how it works. You create `~/.config/binup/config.yaml` with the following contents:

```yaml
tools:
  binup:
    project: KonishchevDmitry/binup
    release_matcher: binup-linux-x64-*
```

... and run `binup install` command to install the specified apps or `binup upgrade` to upgrade already installed apps.

The tool is fully stateless: it doesn't save any information about installed binaries. Instead, is always checks the actual state of the apps: if binary is missing, it installs it. When the binary already installed, it runs it with `--version` argument and tries to parse its actual version to compare with the latest release. If it fails to determine the version (the tool might not have `--version` flag), binup relies on binary file modification time, always setting it to update time of the downloaded release archive.

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

    # Changelog URL (will be printed on app upgrade)
    changelog: https://github.com/prometheus/prometheus/blob/main/CHANGELOG.md

    # Release archive pattern:
    # * By default shell-like glob matching is used (https://docs.rs/globset/latest/globset/#syntax)
    # * Pattern started with '~' is treated as regular expression (https://docs.rs/regex/latest/regex/#syntax)
    release_matcher: prometheus-*.linux-amd64.tar.gz

    # Binary path to look inside the release archive. If it's not specified, the tool name will be used instead.
    binary_matcher: "*/prometheus"

    # Post-install script
    post: systemctl restart prometheus

# If you have a lot of tools, you'll likely hit GitHub API rate limits for anonymous requests at some moment.
# So it's recommended to obtain GitHub token (https://github.com/settings/tokens) and specify it here.
# No permissions are required for the token – it's needed just to make API requests non-anonymous.
github:
  token: $token
```