# Third-party notices

VarKeep v2 uses Windows native controls through [windows-sys](https://github.com/microsoft/windows-rs) 0.61.2. VarKeep's own source is licensed under MIT; the resolved third-party license texts remain included with the distribution.

The complete resolved Rust package inventory, declared license expressions, source repositories, and collected license/notice texts are distributed in `THIRD-PARTY-LICENSES.txt`. The file is generated from the locked Windows x64 dependency graph by `scripts/generate-third-party-licenses.ps1` and checked by the release gate.

VarKeep uses [winresource](https://github.com/BenjaminRi/winresource) 0.1.31 as an MIT-licensed build-time dependency to embed Windows icon and version resources. Build dependencies are included in the generated inventory even though they are not linked into the runtime dependency graph.
