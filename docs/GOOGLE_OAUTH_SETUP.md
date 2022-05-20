# Overview

This guide is for setting up Google SSO on FusionAuth

## Prerequisites

* FusionAuth
* Google Cloud App access (create your own for dev instances)

## Setup GCP Settings

1. Follow the directions listed in the FusionAuth docs to set up a new [Google Cloud App](https://fusionauth.io/docs/v1/tech/identity-providers/google) if this is for a developer environment.
    * If you see an alert indicating you first need to configure the content screen, do that now by clicking on Configure consent screen, External type.
    * Fill out the following fields:
        * App Name
        * User support email
        * App Logo (Required for production)
        * App Domain (Required for production)
        * Application terms of service link (Required for production)
        * Authorized domains (Required for production)
            * squadov.gg (no http, https, or app. prefix required)
        * Developer contact info
    * For developer environments: 
        * Authorized JavaScript origins
            * http://localhost:3000 (or whatever you run your Web Client app port on)
            * http://localhost
    * For production:
        * Authorized JavaScript origins
            * https://app.squadov.gg

    Take note of the "Client ID" and "Client Secret", these will be required for setting up FusionAuth.

2. In FusionAuth, under Identity Providers, click "add provider" and fill out fields for Client ID and Client Secret. Enable the IDP for the SquadOV app.