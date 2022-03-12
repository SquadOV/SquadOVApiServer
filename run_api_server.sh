#!/bin/bash

ulimit -Sn 65535
./squadov_api_server --config ./config/config.toml --workers $WORKERS