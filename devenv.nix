# studio-proto's dev environment: the same as Drive's (devenv-fleet Rust module:
# rust 1.99.1 and kache).
{ pkgs, ... }:
{
  packages = [ pkgs.pkg-config ];
}
