# tauri-static-deployer

modifies the tauri-action workflow to allow for release-channel-per-branch model

## WARNING: this is work in progress, use with caution and report any errors

### notes

- this was only tested on digitalocean spaces (s3), updating to AWS S3 would probably require some fiddling
- only windows was tested - if you need to use this on other platforms please open an issue

### usage

in order for this to work you need to keep updating your `package.version` key in `tauri.conf.json`

add this to your github acition .yml file

```yml
- name: clone deployer script
  run: git clone https://github.com/Grupa-Pieprzyk/tauri-static-deployer ../tauri-static-deployer

- name: rust cache for deployer script
  uses: Swatinem/rust-cache@v1
  with:
    working-directory: ../tauri-static-deployer
    key: ${{ matrix.settings.target }}

- name: install deployer script
  run: |
    cd ../tauri-static-deployer
    cargo build --release
- name: create the static release - update tauri.conf.json
  run: ../tauri-static-deployer/target/release/tauri-static-deployer patch
  env:
    S3_ACCESS_KEY: ${{ secrets.DIGITAL_OCEAN_ACCESS_KEY }}
    S3_SECRET_KEY: ${{ secrets.DIGITAL_OCEAN_SECRET_KEY }}
    S3_BUCKET: change-this-to-your-app-name
    S3_REGION: fra1
    RUST_LOG: info

# --- your tauri action goes here ---
- name: install app dependencies and build it
  run: yarn && yarn build
- uses: tauri-apps/tauri-action@dev
  env:
    GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
    TAURI_PRIVATE_KEY: ${{ secrets.TAURI_PRIVATE_KEY }}
    CARGO_NET_GIT_FETCH_WITH_CLI: true
# --- end of your tauri action ---

- name: create the static release - upload and release new app version
  run: ../tauri-static-deployer/target/release/tauri-static-deployer upload
  env: # WARN: make sure all those envs are the same as above
    S3_ACCESS_KEY: ${{ secrets.DIGITAL_OCEAN_ACCESS_KEY }}
    S3_SECRET_KEY: ${{ secrets.DIGITAL_OCEAN_SECRET_KEY }}
    S3_BUCKET: change-this-to-your-app-name
    S3_REGION: fra1
    RUST_LOG: info
```
