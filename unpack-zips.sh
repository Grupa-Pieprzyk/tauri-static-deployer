#!/usr/bin/env bash
set -e

cd ./bin/
for file in ./*.zip;
  do 
    WITHOUT_ZIP="$(echo "${file%.*}")"
    echo "${file} -> ${WITHOUT_ZIP}"
    yes Y | unzip "${file}" -d "${WITHOUT_ZIP}"

done
