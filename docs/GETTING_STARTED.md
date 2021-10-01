# Getting Started

This tutorial assumes you've setup your machine and installed the pre-requisites for making the SquadOV client as well.

## Prerequisites

From here on out, the `SquadOVApiServer` folder will be referred to as `$SRC`.

* Install [Docker](https://docs.docker.com/docker-for-windows/wsl/)
* Install [Docker Compose](https://docs.docker.com/compose/install/) (follow the instructions for Linux).
* Install [Rust](https://www.rust-lang.org/tools/install)
* Install [Chocolatey](https://chocolatey.org/install#individual)
* Install [Terraform](https://learn.hashicorp.com/tutorials/terraform/install-cli)
* Install [aws CLI](https://docs.aws.amazon.com/cli/latest/userguide/install-cliv2-windows.html)
* Install [jq](https://community.chocolatey.org/packages/jq/1.6)

After installing Rust, run `rustup default 1.51.0`.

You will also need to install additional dependencies in the `deps` folder.

1. `cd $SRC/deps`
2. `.\pull_deps.ps1`
3. Add the following paths to your PATH environment variable:
    * `$SRC/deps/flyway`

## Setting up Infrastructure Services

1. `cd $SRC/config`
2. `cp config.toml.tmpl config.toml`
3. Set the following variables:
   1. `fusionauth.host` to `http://127.0.0.1`
   2. `database.url` to `postgresql://postgres:password@127.0.0.1/squadov`
   3. `cors.domain` to `http://localhost:3000`
4. Copy the appropriate GCS access key into the file `devops\gcp\dev.json`
    * You may need to ask someone for this

### Set up FusionAuth

1. `cd $SRC/devops/docker`
2. `..\env\dev_env.ps1`
3. `docker-compose -f local-dev-compose.yml up`
4. Open up `127.0.0.1:9011` in a browser and setup FusionAuth using the Setup Wizard.
5. Login and create a new application.

    Give the application a reasonable name. Under the Security tab set

    * `Require an API key` to true.
    * `Generate Refresh Tokens` to true.
    * `Enable JWT refresh` to true.

    Hit save.
6. Set `FUSIONAUTH_CLIENT_ID` and `FUSIONAUTH_CLIENT_SECRET` in `$SRC/devops/env/dev_vars.json` to your application's client ID and client secret respectively.
7. Set `fusionauth.application_id` in `$SRC\config\config.toml` to your application's client ID.
8. Set `fusionauth.tenant_id`in `$SRC\config\config.toml` and `FUSIONAUTH_TENANT_ID` in `$SRC/devops/env/dev_vars.json` to the Default tenant ID.
9. Setup a FusionAuth API key.

    Copy the key and modify `FUSIONAUTH_API_KEY` in `$SRC/devops/docker/dev_env.sh` to the key value.
    TODO: Determine minimal set of endpoint permissions.
10. Setup an SMTP server.

    Under General, set:
    * Issuer: squadov.gg

    Under Email, set:

    * Host: smtp.postmarkapp.com
    * Port: 587
    * Security: TLS
    * Username: Postmark API token
    * Password: Postmark API token
    * Verify Email: TRUE
    * Verify email when changed: TRUE
    * Verification template: Email Verification

### Setup PostgreSQL

1. `cd $SRC/devops/database`
2. `.\migrate.ps1 migrate`

### Build and Run

There's a couple of environment variables that need to be set:
* `FFMPEG_BINARY_PATH`: Binary path to a pre-built FFmpeg.

1. `cd $SRC`
2. `.\devops\env\dev_env.ps1`
3. `$env:CMAKE_TOOLCHAIN_FILE="$VCPKG\scripts\buildsystems\vcpkg.cmake"; cargo build --bin squadov_api_server`
4. `$env:FFMPEG_BINARY_PATH="$SquadOVClient\prebuilt\ffmpeg\bin\ffmpeg.exe"; cargo run --bin squadov_api_server -- --config .\config\config.toml`

### AWS
1. Generate an Access Key in AWS IAM Credentials
2. Make sure to save these in ~/.aws/credentials
3. Create [cloudfront pem file](https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/private-content-trusted-signers.html)
4. Store your private key in `$SRC/devops/aws/keys`

### Run Terraform
Use Git Bash

0. Create a new new bucket in S3 for Terraform
1. `cd $SRC/devops/env`
2. `. dev_env.sh`
3. `cd ../terraform-aws`
4. Modify the `run_terraform.sh` script to include the user, and access keys
5. `./run_terraform.sh squadov-dev`

In the off-chance SQLx complains about `gen_random_uuid` not being defined you will have to re-create the `pgcrypto` extension in the PostgreSQL database:

```
DROP EXTENSION pgcrypto CASCADE;
CREATE EXTENSION pgcrypto;

ALTER TABLE squadov.users
ALTER COLUMN uuid SET DEFAULT gen_random_uuid();

ALTER TABLE squadov.squad_membership_invites
ALTER COLUMN invite_uuid SET DEFAULT gen_random_uusid();
```