#!/usr/bin/env bash

set -e

IP=192.168.1.45
USER="niedzwiedz-serwer"
SSH_HOST="${USER}@${IP}"

echo "sending to ${SSH_HOST}"
rsync -rP --exclude "target" ./ "${SSH_HOST}:~/launcher-deployer"
echo "done!"

