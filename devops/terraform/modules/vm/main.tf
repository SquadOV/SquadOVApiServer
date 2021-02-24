resource "google_storage_bucket" "vod-storage-bucket" {
    name            = var.vod_storage_bucket
    location        = "US-CENTRAL1"
    storage_class   = "STANDARD"
}

resource "google_storage_bucket" "blob-storage-bucket" {
    name            = var.blob_storage_bucket
    location        = "US-CENTRAL1"
    storage_class   = "STANDARD"
}

resource "google_project_iam_custom_role" "api-service-custom-role" {
    role_id     = "apiServiceAccountRole"
    title       = "API Service Account Role"
    permissions = ["storage.objects.get", "storage.objects.delete", "storage.objects.list", "storage.objects.create", "storage.buckets.get"]
}

resource "google_service_account" "api-service-account" {
    account_id      = "api-service-account"
    display_name    = "API Service Account"
}

resource "google_service_account_key" "api-service-account-key" {
    service_account_id = google_service_account.api-service-account.name
}

resource "google_project_iam_member" "service-account-role" {
    role = google_project_iam_custom_role.api-service-custom-role.name
    member = "serviceAccount:${google_service_account.api-service-account.email}"
}

resource "local_file" "api-service-account-key-file" {
    filename = var.service_account_key_filename
    content = base64decode(google_service_account_key.api-service-account-key.private_key)
}