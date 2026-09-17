# Changelog

## [0.6.0](https://github.com/o2csi/candeo/compare/v0.5.1...v0.6.0) (2026-09-17)


### Features

* a devices page on the site, with a filter ([499ce50](https://github.com/o2csi/candeo/commit/499ce50794f598120e404453aad3e2d0a4891889))
* a page for the project, at o2csi.github.io/candeo ([ade11ea](https://github.com/o2csi/candeo/commit/ade11ea4d33d05169a6eb3322d08db97bedbebd0))
* automations, rules that interrupt the effect on a device for a while, written in cron ([3b8c8d4](https://github.com/o2csi/candeo/commit/3b8c8d40d8387304fab2b8cb41b289c8c52eea1f))
* Clock, a shipped effect that scrolls the time across the keyboard ([5a8117b](https://github.com/o2csi/candeo/commit/5a8117b73f414cd04ff65e237a10194ad49d077b))
* copy buttons on the site's commands and code ([50b8529](https://github.com/o2csi/candeo/commit/50b85293ec2bdfe33279af7d698675116fda1de4))
* effects can read the clock ([5a8117b](https://github.com/o2csi/candeo/commit/5a8117b73f414cd04ff65e237a10194ad49d077b))


### Documentation

* a privacy policy, which the Store listing asks for ([36beea7](https://github.com/o2csi/candeo/commit/36beea7f71972dc58e09a0e97369deb1f19a0bfd))
* install by the winget moniker ([50b8529](https://github.com/o2csi/candeo/commit/50b85293ec2bdfe33279af7d698675116fda1de4))
* O2CSI links to its site ([9e0a738](https://github.com/o2csi/candeo/commit/9e0a738c2edf753fa8ade3e06b962764ff021296))
* position the site on devices, with a roadmap ([278c526](https://github.com/o2csi/candeo/commit/278c52665b3d30c7d93223eb1c389b3bf02a310c))
* the landing page stops counting devices ([#170](https://github.com/o2csi/candeo/issues/170)) ([446e40a](https://github.com/o2csi/candeo/commit/446e40a107e69ea40066feb0c43747586cde3110))
* the Store listing, in English and French ([e503812](https://github.com/o2csi/candeo/commit/e503812c84359ec61f51b115e90c338cb8f2eae0))

## [0.5.1](https://github.com/o2csi/candeo/compare/v0.5.0...v0.5.1) (2026-09-16)


### Bug Fixes

* the log folder is shown with ~, as every other path ([f14ce77](https://github.com/o2csi/candeo/commit/f14ce7747e62a7ef169289c1fc3dbe35aac9e333))

## [0.5.0](https://github.com/o2csi/candeo/compare/v0.4.0...v0.5.0) (2026-09-16)


### Features

* build the MSIX package for the Store ([0e23ec2](https://github.com/o2csi/candeo/commit/0e23ec2f5942091ffb70d01f92aa187f4792f488))
* launch at login in the Store version ([98ba2dd](https://github.com/o2csi/candeo/commit/98ba2dd73f1d4a2581bc3f8ac79299e43f17693b))
* manifests for the Windows Package Manager ([6bb519e](https://github.com/o2csi/candeo/commit/6bb519e970988d0bca5f7f57ea1afaffcac5c26e))
* tell people when a newer version exists ([b16cefe](https://github.com/o2csi/candeo/commit/b16cefe4815de13d3296b3d75b58a1055224701b))


### Bug Fixes

* escape the identity values written into the manifest ([4adc188](https://github.com/o2csi/candeo/commit/4adc1880141617e4023f85758ac2777e8d782883))
* text that was too pale to read ([db314ab](https://github.com/o2csi/candeo/commit/db314abfee5b05a371931239edb0014402be4c71))
* the startup task types are Windows only ([2a43561](https://github.com/o2csi/candeo/commit/2a435614cb01133b790e2afb970994f339058383))


### Refactoring

* take the repository from Cargo.toml ([91bbb98](https://github.com/o2csi/candeo/commit/91bbb98f3862dc57e2dba79db7ade82351213edf))


### Documentation

* AppData is not redirected in this package ([a63761c](https://github.com/o2csi/candeo/commit/a63761c2978c30b0bb1bed7bb9269a4ff632bc4b))
* the certification kit passes on the reserved identity ([df75d80](https://github.com/o2csi/candeo/commit/df75d80ce29fe1eb363955dbea538a1cdea0ee69))
* the package starts at login through a startup task ([19b8c74](https://github.com/o2csi/candeo/commit/19b8c7441f7fbe3cf0dbcd9f61f19f67e5ed6e3d))
* uninstalling the package leaves the data ([3a71bf3](https://github.com/o2csi/candeo/commit/3a71bf367a175f5403d6ecb5f15d3591055cc822))

## [0.4.0](https://github.com/oorabona/candeo/compare/v0.3.0...v0.4.0) (2026-09-16)


### Features

* name the executable candeo ([93ee21b](https://github.com/oorabona/candeo/commit/93ee21bdb7b78c0ffeadfbb120a1a81589a385f0))


### Bug Fixes

* give the window the icon the taskbar reads ([e73cc8e](https://github.com/oorabona/candeo/commit/e73cc8ef3ad21e978f2d1d5986fa10b3ee6e89de))
* show the application icon on the installer ([cc6a1a1](https://github.com/oorabona/candeo/commit/cc6a1a1278cdc92857a21d0b98d3456055039519))

## [0.3.0](https://github.com/oorabona/candeo/compare/v0.2.0...v0.3.0) (2026-09-15)


### Features

* add the backlit keycap application icon ([d99d4b7](https://github.com/oorabona/candeo/commit/d99d4b73ed2b4ceef7e86ef63044327b415cc449))
* **effects:** number duplicates instead of a translated suffix ([33c436f](https://github.com/oorabona/candeo/commit/33c436fceaf72a8e816356512cbdb1688c097115))


### Bug Fixes

* keep file and folder names lowercase ([0a729a6](https://github.com/oorabona/candeo/commit/0a729a6544ef64c1e956904124dbed9ee9649060))
* **log:** write the home directory as ~ in error messages ([5ea7e05](https://github.com/oorabona/candeo/commit/5ea7e051694cd7c1d492e682426df769d3b63b7c))
* **release:** same shipped bytes on every build, O2CSI as publisher ([e4abb5c](https://github.com/oorabona/candeo/commit/e4abb5c382698dac68539c34723403cdec0178cd))
* spell the product name Candeo everywhere ([9b667cd](https://github.com/oorabona/candeo/commit/9b667cdb8555f516c3b7e90b658b0b041da23cab))


### Refactoring

* drop the diamond from the tab rail ([f34b196](https://github.com/oorabona/candeo/commit/f34b1969228d60a1ce738c07c28a762a9ee5e455))

## [0.2.0](https://github.com/oorabona/candeo/compare/v0.1.0...v0.2.0) (2026-09-15)


### Features

* **effects:** shipped effects in the app folder, yours in Documents ([d709143](https://github.com/oorabona/candeo/commit/d70914323ac5f462d0caa5a82c9ba25b4d56d419)), closes [#125](https://github.com/oorabona/candeo/issues/125)


### Documentation

* effect sources, shipped and the user's ([fa06255](https://github.com/oorabona/candeo/commit/fa062553d91fa7c2ca35d2c543938903c0e80b41)), closes [#125](https://github.com/oorabona/candeo/issues/125)

## 0.1.0 (2026-09-15)


### CI/CD

* release pipeline with release-please ([c5c90e8](https://github.com/oorabona/candeo/commit/c5c90e83830be1974b044e2efd68c3674e5e0819))
