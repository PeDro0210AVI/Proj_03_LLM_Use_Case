{

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-25.11";
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    {
      nixpkgs,
      crane,
      ...
    }:
    let
      forAllSystems =
        function:
        nixpkgs.lib.genAttrs
          [
            "x86_64-linux"
            "aarch64-darwin"
          ]
          (
            system:
            let
              pkgs = import nixpkgs {
                inherit system;
                config = {
                  allowUnfree = true;
                };

              };

              buildInputs = with pkgs; [
                pkg-config
              ];

              nativeBuildInputs = with pkgs; [
                glfw
                cmake
                clang
                cargo
                rustc
              ];

            in
            function {
              inherit
                pkgs
                buildInputs
                nativeBuildInputs
                ;

            }
          );
    in
    {

      # One package per workspace member, built from the shared Cargo.lock.
      packages = forAllSystems (
        {
          pkgs,
          nativeBuildInputs,
          buildInputs,
        }:
        let
          craneLib = crane.mkLib pkgs;
          lib = pkgs.lib;

          commonArgs = {
            inherit nativeBuildInputs buildInputs;
            src = craneLib.cleanCargoSource ./.;
            strictDeps = true;

            env = {
              RUSTFLAGS = "-C link-args=-Wl,-rpath,${lib.makeLibraryPath buildInputs}";
            };
          };

          cargoArtifacts = craneLib.buildDepsOnly (commonArgs // { pname = "corpo-metrics-workspace"; });

          buildMember =
            pname: crate:
            craneLib.buildPackage (
              commonArgs
              // {
                inherit pname cargoArtifacts;
                cargoExtraArgs = "--package ${crate}";
                doCheck = false;
              }
            );
        in
        rec {
          # MCP server (reads ./data relative to its working directory, so
          # run it from inside Corpo-Metrics/).
          corpo-metrics = buildMember "corpo-metrics" "corpo_metrics";
          corpo-metrics-chatbot = buildMember "corpo-metrics-chatbot" "corpo-metrics-chatbot";
          default = corpo-metrics-chatbot;
        }
      );

      devShells = forAllSystems (
        {
          pkgs,
          buildInputs,
          nativeBuildInputs,
        }:
        {
          default =
            let
              LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath buildInputs}";
            in
            pkgs.mkShell {
              inherit nativeBuildInputs buildInputs LD_LIBRARY_PATH;

              packages = with pkgs; [
                cargo
                bacon
                rust-analyzer
                clippy
                rustfmt
                taplo # lsp for cargo.toml

                # local LLM runtime used by the chatbot (swap for
                # `ollama-cuda` / `ollama-rocm` to use a GPU)
                ollama

                claude-code # the demon itself
              ];

            };
        }
      );

    };
}
