terraform {
    backend "gcs" {
        bucket = "squadov-tf-state"
        prefix = "terraform/state"
    }
}

provider "google" {
    project     = "squadov"
    region      = "us-central1"
    zone        = "us-central1-c"
    version     =  "~> 3.7"
    scopes      = [
        "https://www.googleapis.com/auth/compute",
        "https://www.googleapis.com/auth/cloud-platform",
        "https://www.googleapis.com/auth/ndev.clouddns.readwrite",
        "https://www.googleapis.com/auth/devstorage.full_control",
        "https://www.googleapis.com/auth/userinfo.email",
        "https://www.googleapis.com/auth/cloud-platform",
        "https://www.googleapis.com/auth/sqlservice.admin",
    ]
}

module "vm" {
    source = "../modules/vm"

    service_account_key_filename = "../../gcp/squadov.json"
    vod_storage_bucket = var.vod_storage_bucket
    blob_storage_bucket = var.blob_storage_bucket
}