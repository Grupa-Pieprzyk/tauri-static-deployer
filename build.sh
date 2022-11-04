# #!/usr/bin/env bash
# set -e

# WINDOWS_TARGET=x86_64-pc-windows-gnu
# MACOS_TARGET=x86_64-apple-darwin
# LINUX_TARGET=x86_64-unknown-linux-gnu
# # APP_NAME="blazing-packager"
# # TARGET_DIR="./build-tools/bin/"
# export CROSS_BUILD_OPTS="--network host"

# echo setting up macos
# echo "make sure you have completed all the steps from [https://github.com/cross-rs/cross-toolchains#targets]"
# echo "this is also needed [https://github.com/cross-rs/cross-toolchains/issues/17#issuecomment-1282527676]"
# echo "also you need to download a release from [https://github.com/phracker/MacOSX-SDKs]"
# # MACOS_SDK_DIR=/home/niedzwiedz/Programming/MacOSX-SDKs # you probably need to change this to your local path
# # MACOS_SDK_VERSION=MacOSX11.3.sdk
# MACOS_SDK_DIR="./macos-deps"
# MACOS_SDK_FILE="MacOSX11.3.sdk.tar.xz"
# head -c 1 "${MACOS_SDK_DIR}/${MACOS_SDK_FILE}"
# # MACOS_SDK_URL=https://github.com/phracker/MacOSX-SDKs/releases/download/11.3/MacOSX11.3.sdk.tar.xz
# MACOS_SDK_URL="https://objects.githubusercontent.com/github-production-release-asset-2e65be/13597203/d8bf0a00-ac28-11eb-8773-a200eff1c463?X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=AKIAIWNJYAX4CSVEH53A%2F20221104%2Fus-east-1%2Fs3%2Faws4_request&X-Amz-Date=20221104T175154Z&X-Amz-Expires=300&X-Amz-Signature=4d6b2f0b6223ce2b243763877deda3c292a4274f576d8deaccdc7f8bbff8be10&X-Amz-SignedHeaders=host&actor_id=0&key_id=0&repo_id=13597203&response-content-disposition=attachment%3B%20filename%3DMacOSX11.3.sdk.tar.xz&response-content-type=application%2Foctet-stream"
# CROSS_DIRECTORY=/home/niedzwiedz/Programming/cross # and this one too
# # rustup target add "${MACOS_TARGET}"


# export CROSS_CONTAINER_ENGINE_NO_BUILDKIT=1 # buildkit doesnt work

# if [ "${1}" == "--rebuild-macos" ];
# then
# echo building macos docker image with sdk
# cd "${CROSS_DIRECTORY}"
#  # --build-arg "MACOS_SDK_DIR=${MACOS_SDK_DIR}" \
#  # --build-arg "MACOS_SDK_FILE=${MACOS_SDK_FILE}" \
# ./build_clang.sh
# UNATTENDED=1 ./build.sh
# cargo build-docker-image "${MACOS_TARGET}-cross" \
#   --build-arg "MACOS_SDK_URL=${MACOS_SDK_URL}" \
#   --tag local
# cd -
# fi

# echo compiling windows
# rustup target add "${WINDOWS_TARGET}"
# cross build --release --target="${MACOS_TARGET}"
# cross build --release --target="${WINDOWS_TARGET}"
# cross build --release --target="${LINUX_TARGET}"
