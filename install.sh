#!/bin/bash

mkdir -p ~/.config/systemd/user
cp open-coreui.service ~/.config/systemd/user/

systemctl --user daemon-reload

systemctl --user enable open-coreui.service

systemctl --user restart open-coreui.service

systemctl --user status open-coreui.service

journalctl --user -u open-coreui.service -f