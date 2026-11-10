# Changelog

## [0.3.0](https://github.com/DevSecNinja/note-iq/compare/v0.2.2...v0.3.0) (2026-11-10)


### Features

* **release:** link downloads in the release notes and publish per-file checksums ([#21](https://github.com/DevSecNinja/note-iq/issues/21)) ([236ccc1](https://github.com/DevSecNinja/note-iq/commit/236ccc1df33267fca1123343a11ecb703c430311))

## [0.2.2](https://github.com/DevSecNinja/note-iq/compare/v0.2.1...v0.2.2) (2026-11-09)


### Bug Fixes

* **release:** add a serial number so actions/attest accepts the SBOMs ([11172a2](https://github.com/DevSecNinja/note-iq/commit/11172a23aaa3ca9b24c1872b199abce4777ab768))
* **release:** add a serial number so actions/attest accepts the SBOMs ([477dbe6](https://github.com/DevSecNinja/note-iq/commit/477dbe6aa5382c81adb2150eea1b53bfb076cf4f))

## [0.2.1](https://github.com/DevSecNinja/note-iq/compare/v0.2.0...v0.2.1) (2026-11-09)


### Bug Fixes

* **release:** exclude proc-macro dependencies from release SBOMs ([dbf9d90](https://github.com/DevSecNinja/note-iq/commit/dbf9d90a275639a2a387e8b63c93819dea981f1f))
* **release:** exclude proc-macro dependencies from release SBOMs ([f74a06d](https://github.com/DevSecNinja/note-iq/commit/f74a06d9281a30ccc1b0b1e041d7443911ead139))

## [0.2.0](https://github.com/DevSecNinja/note-iq/compare/v0.1.0...v0.2.0) (2026-11-09)


### Features

* **release:** publish CycloneDX SBOMs with attestations ([81f1ce5](https://github.com/DevSecNinja/note-iq/commit/81f1ce5e03aa1bf0fed068989cb3892ca68641f3))
* **release:** publish CycloneDX SBOMs with attestations ([643ac59](https://github.com/DevSecNinja/note-iq/commit/643ac59a78cd662ce4deef52cea1ac3248e11573))
* **ui:** simplify the capture window ([fec416a](https://github.com/DevSecNinja/note-iq/commit/fec416a8505ac83e172d6bf56262a3cc8801a6d8))
* **ui:** simplify the capture window ([66b0925](https://github.com/DevSecNinja/note-iq/commit/66b0925198f24ba944b581e8f9f14427cda7b47f))


### Bug Fixes

* make Windows rebuilds safe and checkout-aware ([9b56c16](https://github.com/DevSecNinja/note-iq/commit/9b56c16604c56646f9893f9206d88279c767a096))
* make Windows rebuilds safe and checkout-aware ([e85a8dc](https://github.com/DevSecNinja/note-iq/commit/e85a8dcedcd1bb60e22f57020435d84fd6fcc0b2))
* **ui:** keep the meeting list's keyboard highlight safe and accessible ([2a6b9ed](https://github.com/DevSecNinja/note-iq/commit/2a6b9ed693b71736e416252263a9bfcac18e1e62))

## 0.1.0 (2026-11-08)


### Features

* add NoteIQ quick-capture app ([75cf502](https://github.com/DevSecNinja/note-iq/commit/75cf5029a474788cd0f5c52de414c1a3407f86d8))
* confirm before changing a saved note's meeting ([dbcb715](https://github.com/DevSecNinja/note-iq/commit/dbcb715968b5ccb9f5189ed8aee81f81982a0ab3))
* confirm before changing a saved note's meeting ([d135f4c](https://github.com/DevSecNinja/note-iq/commit/d135f4c1a5f68539b95715b9bdc97f30b2da0bcf))
* refine NoteIQ capture experience and add Windows release pipelines ([eabf679](https://github.com/DevSecNinja/note-iq/commit/eabf67940b5aaa5354b018630e2161c548060952))
* reopen saved notes by meeting occurrence ([24d65e8](https://github.com/DevSecNinja/note-iq/commit/24d65e893d851d0cc93bf9a119f5352227360cee))
* reopen saved notes by meeting occurrence ([3ccf256](https://github.com/DevSecNinja/note-iq/commit/3ccf256c21069e12487042a80a86510a5273c8ed))
* ship NoteIQ capture app with x64 and ARM64 CI/CD ([7f0dfde](https://github.com/DevSecNinja/note-iq/commit/7f0dfdeea469703fe590c4f84a7e219e074021f3))


### Bug Fixes

* address capture reliability and Windows review findings ([412bd21](https://github.com/DevSecNinja/note-iq/commit/412bd214f683f2d3b8d38e9310263c0568a18f52))
* address Copilot review reliability findings ([343748f](https://github.com/DevSecNinja/note-iq/commit/343748f1e3d3fb498a927ffafe0069b5f3bf3fb6))
* address meeting confirmation review feedback ([def351f](https://github.com/DevSecNinja/note-iq/commit/def351f041ef962da6a780ea2c99cbc1956c907c))
* **ci:** declare PowerShell for Windows matrix jobs ([8c1278c](https://github.com/DevSecNinja/note-iq/commit/8c1278ce71ae8cdb9031ff29132da1e57c4744f7))
* **ci:** install shared lint tools and correct repository checks ([002c94a](https://github.com/DevSecNinja/note-iq/commit/002c94a10260d69ff6954e43e8449924d003bbfc))
* **ci:** pass repository-root labeler configuration paths ([642fb79](https://github.com/DevSecNinja/note-iq/commit/642fb79832995048e13deb4c73b8e0032dba945a))
* **core:** accept the canonical noteiq://new/ launch URI ([04ada5c](https://github.com/DevSecNinja/note-iq/commit/04ada5c2d05a4f9c8763444692e0b28caea248d8))
* **core:** back off autosave retries after a failed save ([382314b](https://github.com/DevSecNinja/note-iq/commit/382314bf0b7083f060c6504fb01c963b00d9414b))
* enforce Markdown formatting before commits ([a2d3c98](https://github.com/DevSecNinja/note-iq/commit/a2d3c98e9cd7ab6ea924c9288196f76742546e80))
* preserve keyboard input and make capture notes editable ([e6827ee](https://github.com/DevSecNinja/note-iq/commit/e6827ee80d2bcfd8043a28d4f36e0066725c9f59))
* preserve keyboard input and make capture notes editable ([664e72d](https://github.com/DevSecNinja/note-iq/commit/664e72dcc3c1c5443e15ff761f4cde9e940c39f6))
* preserve reopened note metadata and lookup errors ([fb6beb6](https://github.com/DevSecNinja/note-iq/commit/fb6beb652a9d42bfc4fd419c0631ee74420aa27e))
