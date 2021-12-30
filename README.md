# pollenwall

<details>
  <summary>Table of Contents</summary>

- [pollenwall](#pollenwall)
  - [About](#about)
  - [Support](#support)
  - [Installation](#installation)
    - [Binary releases](#binary-releases)
    - [Build from source](#build-from-source)
  - [Usage](#usage)
    - [Command Line Arguments](#command-line-arguments)
  - [App Folder](#app-folder)
  - [Road Map](#road-map)
  - [Changelog](#changelog)
  - [License](#license)
  - [Copyright](#copyright)

</details>

## About

**pollenwall** is a little command line app which sets your wallpaper with incoming [CLIP-Guided VQGAN](https://pollinations.ai/p/QmScDZ61x3AQbV6NJ9Wxb6VDTfprkPMRGyWM25iQ2xgFiA/create) pollens once they arrive from [pollinations.ai](https://pollinations.ai/c/Anything).

## Support

Currently `pollenwall` only supports x86 and ARM mac computers.
Support will be expanded in the near future.

## Installation

After downloading, add it to an appropriate place in your file system which is included in your `PATH` variable and run `pollenwall` in command line.

If you wish you can navigate to the folder where `pollenwall` is and run it like the following `./pollenwall`.

### Binary releases

Check the [releases page](https://github.com/pollinations/pollenwall/releases) to download `pollenwall`.

### Build from source

You need [Rust](https://www.rust-lang.org/tools/install) to be installed on your system to build `pollenwall`.

For doing a debug build just run `cargo run` in repository root.

For building a fat binary for mac, either run `just build_mac` if you have [just](https://github.com/casey/just) installed or wun the shell script manually like the following `./build_mac.sh`.

## Usage

Just run the app, it is as simple as that.

### Command Line Arguments

```
pollenwall [FLAGS] [OPTIONS]
```

**Flags:**

```
-c, --clean      Remove pollens in "~/.pollen_wall" directory.
-h, --help       Prints help information
-V, --version    Prints version information
```

**Options:**

```
-a, --address <addr>    You may give a custom address to pollinations ipfs node.
    --home <home>       If "pollen_wall" couldn't determine your home directory, to help it please run it with
                        "--home <absolute-path-to-your-home-directory>"
```

## App Folder

App folder where `pollenwall` stores the pollens is located in your home directory with the name `.pollen_wall`.

## Road Map

- [ ] Download other artifacts about a done pollen
- [ ] Show processing pollens in the app output
- [ ] Make pollen storage volatile
- [ ] Support Windows
- [ ] Support Linux
- [ ] Give granular options about which wallpapers to set
- [ ] Support video or `GIF` wall papers in supported platforms
- [ ] Add a run at startup option
- [ ] Publish to package managers for easy installation

## Changelog

All notable changes to this project will be documented under this part.

## License

[MIT License](https://en.wikipedia.org/wiki/MIT_License)

## Copyright

Copyright © 2021, [pollinations-contributors](https://github.com/orgs/pollinations/people)
