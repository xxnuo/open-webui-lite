#!/bin/bash

mkdir -p ~/.config/systemd/user
cp open-webui-lite.service ~/.config/systemd/user/

systemctl --user daemon-reload

systemctl --user enable open-webui-lite.service

systemctl --user restart open-webui-lite.service

systemctl --user status open-webui-lite.service

journalctl --user -u open-webui-lite.service -f