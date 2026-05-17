{
  inputs = {
    nixpkgs.follows = "esp-rust/nixpkgs";

    # hsel-netsys/nixpkgs-esp-dev-rust
    esp-rust.url = "github:waynevanson/nixpkgs-esp-dev";
    esp-dev.url = "github:mirrexagon/nixpkgs-esp-dev";
  };

  outputs =
    inputs:
    let
      system = "x86_64-linux";
      pkgs = import inputs.nixpkgs { inherit system; };
      espRustPkgs = inputs.esp-rust.packages.${system};
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        inputsFrom = [ inputs.esp-dev.devShells.${system}.esp-idf-full ];

        packages = with pkgs; [
          espRustPkgs.rust
          espRustPkgs.llvm
          espRustPkgs.libllvm

          rust-analyzer
          cargo-edit
          cargo-deny
          esp-generate
          espflash
        ];

        # Required by rust-analyzer
        env.RUST_SRC_PATH = "${espRustPkgs.rust}/lib/rustlib/src/rust/library";
      };
    };
}
