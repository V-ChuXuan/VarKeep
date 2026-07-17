# Third-party notices

VarKeep v2 uses [Slint](https://slint.dev/) 1.17.1 under the **Slint Royalty-free Desktop, Mobile, and Web Applications License, Version 2.0**. The application provides the required Slint attribution through the official `AboutSlint` widget on its top-level About screen.

The complete resolved Rust package inventory, declared license expressions, source repositories, and collected license/notice texts are distributed in `THIRD-PARTY-LICENSES.txt`. The file is generated from the locked Windows x64 dependency graph by `scripts/generate-third-party-licenses.ps1` and checked by the release gate.

VarKeep uses [winresource](https://github.com/BenjaminRi/winresource) 0.1.31 as an MIT-licensed build-time dependency to embed Windows icon and version resources. Build dependencies are included in the generated inventory even though they are not linked into the runtime dependency graph.
