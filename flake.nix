{
  description = "A leading-edge control system for quantum information experiments";

  inputs = {
    nac3 = {
      url = "/home/esavkin/CLionProjects/nac3/";
    };

    rust-overlay = {
      url = "github:oxalica/rust-overlay?ref=snapshot/2024-08-01";
      inputs.nixpkgs.follows = "nac3/nixpkgs";
    };

    artiq-comtools = {
      url = "github:m-labs/artiq-comtools";
      inputs.nixpkgs.follows = "nac3/nixpkgs";
      inputs.sipyco.follows = "sipyco";
    };

    sipyco = {
      url = "github:m-labs/sipyco";
      inputs.nixpkgs.follows = "nac3/nixpkgs";
    };

    src-migen = {
      url = "github:m-labs/migen";
      flake = false;
    };

    src-misoc = {
      url = "https://github.com/m-labs/misoc.git";
      type = "git";
      submodules = true;
      flake = false;
    };
  };

  outputs = {
    self,
    rust-overlay,
    sipyco,
    nac3,
    artiq-comtools,
    src-migen,
    src-misoc,
  }: let
    pkgs = import nac3.inputs.nixpkgs {
      system = "x86_64-linux";
      overlays = [(import rust-overlay)];
    };
    pkgs-aarch64 = import nac3.inputs.nixpkgs {system = "aarch64-linux";};

    artiqVersionMajor = 10;
    artiqVersionMinor = self.sourceInfo.revCount or 0;
    artiqVersionId = self.sourceInfo.shortRev or "unknown";
    artiqVersion = (builtins.toString artiqVersionMajor) + "." + (builtins.toString artiqVersionMinor) + "+" + artiqVersionId + ".beta";
    artiqRev = self.sourceInfo.rev or "unknown";

    qtPaths = let
      inherit (pkgs.qt6) qtbase qtsvg;
      inherit (qtbase.dev) qtPluginPrefix qtQmlPrefix;
    in {
      QT_PLUGIN_PATH = "${qtbase}/${qtPluginPrefix}:${qtsvg}/${qtPluginPrefix}";
      QML2_IMPORT_PATH = "${qtbase}/${qtQmlPrefix}";
    };

    rust = pkgs.rust-bin.nightly."2021-09-01".default.override {
      extensions = ["rust-src"];
      targets = [];
    };
    rustPlatform = pkgs.makeRustPlatform {
      rustc = rust;
      cargo = rust;
    };

    vivadoDeps = pkgs:
      with pkgs; let
        # Apply patch from https://github.com/nix-community/nix-environments/pull/54
        # to fix ncurses libtinfo.so's soname issue
        ncurses' = ncurses5.overrideAttrs (old: {
          configureFlags = old.configureFlags ++ ["--with-termlib"];
          postFixup = "";
        });
      in [
        libxcrypt-legacy
        (ncurses'.override {unicodeSupport = false;})
        zlib
        libuuid
        xorg.libSM
        xorg.libICE
        xorg.libXrender
        xorg.libX11
        xorg.libXext
        xorg.libXtst
        xorg.libXi
        freetype
        fontconfig
      ];

    qasync = pkgs.python3Packages.buildPythonPackage rec {
      pname = "qasync";
      version = "0.27.1";
      format = "pyproject";
      src = pkgs.fetchFromGitHub {
        owner = "CabbageDevelopment";
        repo = "qasync";
        rev = "refs/tags/v${version}";
        sha256 = "sha256-oXzwilhJ1PhodQpOZjnV9gFuoDy/zXWva9LhhK3T00g=";
      };
      postPatch = ''
        rm qasync/_windows.py # Ignoring it is not taking effect and it will not be used on Linux
      '';
      buildInputs = [pkgs.python3Packages.poetry-core];
      propagatedBuildInputs = [pkgs.python3Packages.pyqt6];
      checkInputs = [pkgs.python3Packages.pytestCheckHook];
      pythonImportsCheck = ["qasync"];
      disabledTestPaths = ["tests/test_qeventloop.py"];
    };

    artiq-upstream = pkgs.python3Packages.buildPythonPackage rec {
      pname = "artiq";
      version = artiqVersion;
      src = self;

      preBuild = ''
        export VERSIONEER_OVERRIDE=${version}
        export VERSIONEER_REV=${artiqRev}
      '';

      nativeBuildInputs = [pkgs.qt6.wrapQtAppsHook];

      # keep llvm_x in sync with nac3
      propagatedBuildInputs =
        [pkgs.llvm_14 nac3.packages.x86_64-linux.nac3artiq-pgo sipyco.packages.x86_64-linux.sipyco pkgs.qt6.qtsvg artiq-comtools.packages.x86_64-linux.artiq-comtools]
        ++ (with pkgs.python3Packages; [pyqtgraph pygit2 numpy dateutil scipy prettytable pyserial h5py pyqt6 qasync tqdm lmdb jsonschema platformdirs]);

      dontWrapQtApps = true;
      postFixup = ''
        wrapQtApp "$out/bin/artiq_dashboard"
        wrapQtApp "$out/bin/artiq_browser"
        wrapQtApp "$out/bin/artiq_session"
      '';

      preFixup = ''
        # Ensure that wrapProgram uses makeShellWrapper rather than makeBinaryWrapper
        # brought in by wrapQtAppsHook. Only makeShellWrapper supports --run.
        wrapProgram() { wrapProgramShell "$@"; }
      '';
      ## Modifies PATH to pass the wrapped python environment (i.e. python3.withPackages(...) to subprocesses.
      ## Allows subprocesses using python to find all packages you have installed
      makeWrapperArgs = [
        ''--run 'if [ ! -z "$NIX_PYTHONPREFIX" ]; then export PATH=$NIX_PYTHONPREFIX/bin:$PATH;fi' ''
        "--set FONTCONFIG_FILE ${pkgs.fontconfig.out}/etc/fonts/fonts.conf"
      ];

      # FIXME: automatically propagate llvm_x dependency
      # cacert is required in the check stage only, as certificates are to be
      # obtained from system elsewhere
      nativeCheckInputs = [pkgs.llvm_14 pkgs.cacert];
      checkPhase = ''
        python -m unittest discover -v artiq.test
      '';
    };

    artiq =
      artiq-upstream
      // {
        withExperimentalFeatures = features: artiq-upstream.overrideAttrs (oa: {patches = map (f: ./experimental-features/${f}.diff) features;});
      };

    migen = pkgs.python3Packages.buildPythonPackage rec {
      name = "migen";
      src = src-migen;
      format = "pyproject";
      nativeBuildInputs = [pkgs.python3Packages.setuptools];
      propagatedBuildInputs = [pkgs.python3Packages.colorama];
    };

    asyncserial = pkgs.python3Packages.buildPythonPackage rec {
      pname = "asyncserial";
      version = "1.0";
      src = pkgs.fetchFromGitHub {
        owner = "m-labs";
        repo = "asyncserial";
        rev = version;
        sha256 = "sha256-ZHzgJnbsDVxVcp09LXq9JZp46+dorgdP8bAiTB59K28=";
      };
      propagatedBuildInputs = [pkgs.python3Packages.pyserial];
    };

    misoc = pkgs.python3Packages.buildPythonPackage {
      name = "misoc";
      src = src-misoc;
      propagatedBuildInputs = with pkgs.python3Packages; [jinja2 numpy migen pyserial asyncserial];
    };

    microscope = pkgs.python3Packages.buildPythonPackage rec {
      pname = "microscope";
      version = "unstable-2020-12-28";
      src = pkgs.fetchFromGitHub {
        owner = "m-labs";
        repo = "microscope";
        rev = "c21afe7a53258f05bde57e5ebf2e2761f3d495e4";
        sha256 = "sha256-jzyiLRuEf7p8LdhmZvOQj/dyQx8eUE8p6uRlwoiT8vg=";
      };
      propagatedBuildInputs = with pkgs.python3Packages; [pyserial prettytable msgpack migen];
    };

    vivadoEnv = pkgs.buildFHSEnv {
      name = "vivado-env";
      targetPkgs = vivadoDeps;
    };

    vivado = pkgs.buildFHSEnv {
      name = "vivado";
      targetPkgs = vivadoDeps;
      profile = "set -e; source /opt/Xilinx/Vivado/2022.2/settings64.sh";
      runScript = "vivado";
    };

    makeArtiqBoardPackage = {
      target,
      variant,
      buildCommand ? "python -m artiq.gateware.targets.${target} -V ${variant}",
      experimentalFeatures ? [],
    }:
      pkgs.stdenv.mkDerivation {
        name = "artiq-board-${target}-${variant}";
        phases = ["buildPhase" "checkPhase" "installPhase"];
        cargoDeps = rustPlatform.importCargoLock {
          lockFile = ./artiq/firmware/Cargo.lock;
          outputHashes = {
            "fringe-1.2.1" = "sha256-u7NyZBzGrMii79V+Xs4Dx9tCpiby6p8IumkUl7oGBm0=";
            "tar-no-std-0.1.8" = "sha256-xm17108v4smXOqxdLvHl9CxTCJslmeogjm4Y87IXFuM=";
          };
        };
        nativeBuildInputs = [
          (pkgs.python3.withPackages (ps: [migen misoc (artiq.withExperimentalFeatures experimentalFeatures) ps.packaging]))
          rust
          pkgs.llvmPackages_14.clang-unwrapped
          pkgs.llvm_14
          pkgs.lld_14
          vivado
          rustPlatform.cargoSetupHook
        ];
        buildPhase = ''
          ARTIQ_PATH=`python -c "import artiq; print(artiq.__path__[0])"`
          ln -s $ARTIQ_PATH/firmware/Cargo.lock .
          cargoSetupPostUnpackHook
          cargoSetupPostPatchHook
          ${buildCommand}
        '';
        doCheck = true;
        checkPhase = ''
          # Search for PCREs in the Vivado output to check for errors
          check_log() {
            grep -Pe "$1" artiq_${target}/${variant}/gateware/vivado.log && exit 1 || true
          }
          check_log "\d+ constraint not met\."
          check_log "Timing constraints are not met\."
        '';
        installPhase = ''
          mkdir $out
          cp artiq_${target}/${variant}/gateware/top.bit $out
          if [ -e artiq_${target}/${variant}/software/bootloader/bootloader.bin ]
          then cp artiq_${target}/${variant}/software/bootloader/bootloader.bin $out
          fi
          if [ -e artiq_${target}/${variant}/software/runtime ]
          then cp artiq_${target}/${variant}/software/runtime/runtime.{elf,fbi} $out
          else cp artiq_${target}/${variant}/software/satman/satman.{elf,fbi} $out
          fi

          mkdir $out/nix-support
          for i in $out/*.*; do
          echo file binary-dist $i >> $out/nix-support/hydra-build-products
          done
        '';
        # don't mangle ELF files as they are not for NixOS
        dontFixup = true;
      };

    openocd-bscanspi-f = pkgs: let
      bscan_spi_bitstreams-pkg = pkgs.stdenv.mkDerivation {
        name = "bscan_spi_bitstreams";
        src = pkgs.fetchFromGitHub {
          owner = "quartiq";
          repo = "bscan_spi_bitstreams";
          rev = "01d8f819f15baf9a8cc5d96945a51e4d267ff564";
          sha256 = "1zqv47kzgvbn4c8cr019a6wcja7gn5h1z4kvw5bhpc72fyhagal9";
        };
        phases = ["installPhase"];
        installPhase = ''
          mkdir -p $out/share/bscan-spi-bitstreams
          cp $src/*.bit $out/share/bscan-spi-bitstreams
        '';
      };
    in
      pkgs.buildEnv {
        name = "openocd-bscanspi";
        paths = [pkgs.openocd bscan_spi_bitstreams-pkg];
      };

    latex-artiq-manual = pkgs.texlive.combine {
      inherit
        (pkgs.texlive)
        scheme-basic
        latexmk
        cmap
        collection-fontsrecommended
        fncychap
        titlesec
        tabulary
        varwidth
        framed
        fancyvrb
        float
        wrapfig
        parskip
        upquote
        capt-of
        needspace
        etoolbox
        booktabs
        pgf
        pgfplots
        ;
    };

    artiq-frontend-dev-wrappers =
      pkgs.runCommandNoCC "artiq-frontend-dev-wrappers" {}
      ''
        mkdir -p $out/bin
        for program in ${self}/artiq/frontend/*.py; do
          if [ -x $program ]; then
            progname=`basename -s .py $program`
            outname=$out/bin/$progname
            echo "#!${pkgs.bash}/bin/bash" >> $outname
            echo "exec python3 -m artiq.frontend.$progname \"\$@\"" >> $outname
            chmod 755 $outname
          fi
        done
      '';
  in rec {
    packages.x86_64-linux = rec {
      inherit (nac3.packages.x86_64-linux) python3-mimalloc;
      inherit qasync artiq;
      inherit migen misoc asyncserial microscope vivadoEnv vivado;
      openocd-bscanspi = openocd-bscanspi-f pkgs;
      artiq-board-kc705-nist_clock = makeArtiqBoardPackage {
        target = "kc705";
        variant = "nist_clock";
      };
      artiq-board-efc-shuttler = makeArtiqBoardPackage {
        target = "efc";
        variant = "shuttler";
      };
      inherit latex-artiq-manual;
      artiq-manual-html = pkgs.stdenvNoCC.mkDerivation rec {
        name = "artiq-manual-html-${version}";
        version = artiqVersion;
        src = self;
        buildInputs = with pkgs.python3Packages;
          [
            sphinx
            sphinx_rtd_theme
            sphinxcontrib-tikz
            sphinx-argparse
            sphinxcontrib-wavedrom
          ]
          ++ [
            latex-artiq-manual
            artiq-comtools.packages.x86_64-linux.artiq-comtools
            pkgs.pdf2svg
          ];
        buildPhase = ''
          export VERSIONEER_OVERRIDE=${artiqVersion}
          export SOURCE_DATE_EPOCH=${builtins.toString self.sourceInfo.lastModified}
          cd doc/manual
          make html
        '';
        installPhase = ''
          cp -r _build/html $out
          mkdir $out/nix-support
          echo doc manual $out index.html >> $out/nix-support/hydra-build-products
        '';
      };
      artiq-manual-pdf = pkgs.stdenvNoCC.mkDerivation rec {
        name = "artiq-manual-pdf-${version}";
        version = artiqVersion;
        src = self;
        buildInputs = with pkgs.python3Packages;
          [
            sphinx
            sphinx_rtd_theme
            sphinxcontrib-tikz
            sphinx-argparse
            sphinxcontrib-wavedrom
          ]
          ++ [
            latex-artiq-manual
            artiq-comtools.packages.x86_64-linux.artiq-comtools
            pkgs.pdf2svg
          ];
        buildPhase = ''
          export VERSIONEER_OVERRIDE=${artiq.version}
          export SOURCE_DATE_EPOCH=${builtins.toString self.sourceInfo.lastModified}
          cd doc/manual
          make latexpdf
        '';
        installPhase = ''
          mkdir $out
          cp _build/latex/ARTIQ.pdf $out
          mkdir $out/nix-support
          echo doc-pdf manual $out ARTIQ.pdf >> $out/nix-support/hydra-build-products
        '';
      };
    };

    packages.x86_64-w64-mingw32 = import ./windows {
      inherit sipyco nac3 artiq-comtools;
      artiq = self;
    };

    inherit qtPaths makeArtiqBoardPackage openocd-bscanspi-f;

    defaultPackage.x86_64-linux = packages.x86_64-linux.python3-mimalloc.withPackages (_: [packages.x86_64-linux.artiq]);

    formatter.x86_64-linux = pkgs.alejandra;

    devShells.x86_64-linux = {
      # Main development shell with everything you need to develop ARTIQ on Linux.
      # The current copy of the ARTIQ sources is added to PYTHONPATH so changes can be tested instantly.
      # Additionally, executable wrappers that import the current ARTIQ sources for the ARTIQ frontends
      # are added to PATH.
      default = pkgs.mkShell {
        name = "artiq-dev-shell";
        packages = with pkgs;
          [
            git
            lit
            lld_14
            llvm_14
            llvmPackages_14.clang-unwrapped
            pdf2svg

            python3Packages.sphinx
            python3Packages.sphinx-argparse
            python3Packages.sphinxcontrib-tikz
            python3Packages.sphinxcontrib-wavedrom
            python3Packages.sphinx_rtd_theme

            (packages.x86_64-linux.python3-mimalloc.withPackages (ps: [migen misoc microscope ps.packaging] ++ artiq.propagatedBuildInputs))
          ]
          ++ [
            latex-artiq-manual
            rust
            artiq-frontend-dev-wrappers

            # use the vivado-env command to enter a FHS shell that lets you run the Vivado installer
            packages.x86_64-linux.vivadoEnv
            packages.x86_64-linux.vivado
            packages.x86_64-linux.openocd-bscanspi
          ];
        shellHook = ''
          export QT_PLUGIN_PATH=${qtPaths.QT_PLUGIN_PATH}
          export QML2_IMPORT_PATH=${qtPaths.QML2_IMPORT_PATH}
          export PYTHONPATH=`git rev-parse --show-toplevel`:$PYTHONPATH
        '';
      };
      # Lighter development shell optimized for building firmware and flashing boards.
      boards = pkgs.mkShell {
        name = "artiq-boards-shell";
        packages = [
          rust

          pkgs.llvmPackages_14.clang-unwrapped
          pkgs.llvm_14
          pkgs.lld_14

          packages.x86_64-linux.vivado
          packages.x86_64-linux.openocd-bscanspi

          (pkgs.python3.withPackages (ps: [migen misoc artiq ps.packaging]))
        ];
      };
    };

    packages.aarch64-linux = {
      openocd-bscanspi = openocd-bscanspi-f pkgs-aarch64;
    };

    hydraJobs = {
      inherit (packages.x86_64-linux) artiq artiq-board-kc705-nist_clock artiq-board-efc-shuttler openocd-bscanspi;
      sipyco-msys2-pkg = packages.x86_64-w64-mingw32.sipyco-pkg;
      artiq-comtools-msys2-pkg = packages.x86_64-w64-mingw32.artiq-comtools-pkg;
      artiq-msys2-pkg = packages.x86_64-w64-mingw32.artiq-pkg;
      msys2-repos = packages.x86_64-w64-mingw32.msys2-repos;
      inherit (packages.x86_64-linux) artiq-manual-html artiq-manual-pdf;
      gateware-sim = pkgs.stdenvNoCC.mkDerivation {
        name = "gateware-sim";
        buildInputs = [
          (pkgs.python3.withPackages (ps: with packages.x86_64-linux; [migen misoc artiq]))
        ];
        phases = ["buildPhase"];
        buildPhase = ''
          python -m unittest discover -v artiq.gateware.test
          touch $out
        '';
      };
      kc705-hitl = pkgs.stdenvNoCC.mkDerivation {
        name = "kc705-hitl";

        __networked = true; # compatibility with old patched Nix
        # breaks hydra, https://github.com/NixOS/hydra/issues/1216
        #__impure = true;     # Nix 2.8+

        buildInputs = [
          (pkgs.python3.withPackages (
            ps:
              with packages.x86_64-linux;
                [
                  artiq
                  ps.paramiko
                ]
                ++ ps.paramiko.optional-dependencies.ed25519
          ))
          pkgs.llvm_14
          pkgs.openssh
          packages.x86_64-linux.openocd-bscanspi # for the bscanspi bitstreams
        ];
        phases = ["buildPhase"];
        buildPhase = ''
          export HOME=`mktemp -d`
          mkdir $HOME/.ssh
          cp /opt/hydra_id_ed25519 $HOME/.ssh/id_ed25519
          cp /opt/hydra_id_ed25519.pub $HOME/.ssh/id_ed25519.pub
          echo "rpi-1 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIACtBFDVBYoAE4fpJCTANZSE0bcVpTR3uvfNvb80C4i5" > $HOME/.ssh/known_hosts
          chmod 600 $HOME/.ssh/id_ed25519
          LOCKCTL=$(mktemp -d)
          mkfifo $LOCKCTL/lockctl

          cat $LOCKCTL/lockctl | ${pkgs.openssh}/bin/ssh \
            -i $HOME/.ssh/id_ed25519 \
            -o UserKnownHostsFile=$HOME/.ssh/known_hosts \
            rpi-1 \
            'mkdir -p /tmp/board_lock && flock /tmp/board_lock/kc705-1 -c "echo Ok; cat"' \
          | (
            # End remote flock via FIFO
            atexit_unlock() {
              echo > $LOCKCTL/lockctl
            }
            trap atexit_unlock EXIT

            # Read "Ok" line when remote successfully locked
            read LOCK_OK

            artiq_flash -t kc705 -H rpi-1 -d ${packages.x86_64-linux.artiq-board-kc705-nist_clock}
            sleep 30

            export ARTIQ_ROOT=`python -c "import artiq; print(artiq.__path__[0])"`/examples/kc705_nist_clock
            export ARTIQ_LOW_LATENCY=1
            python -m unittest discover -v artiq.test.coredevice
          )

          touch $out
        '';
      };
    };
  };

  nixConfig = {
    extra-trusted-public-keys = "nixbld.m-labs.hk-1:5aSRVA5b320xbNvu30tqxVPXpld73bhtOeH6uAjRyHc=";
    extra-substituters = "https://nixbld.m-labs.hk";
    extra-sandbox-paths = "/opt";
  };
}
