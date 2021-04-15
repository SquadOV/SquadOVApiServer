resource "google_sql_database_instance" "main-db" {
    name = var.postgres_instance_name
    database_version = "POSTGRES_12"
    region = "us-central1"

    settings {
        tier = "db-custom-4-20480"
        availability_type = "ZONAL"

        backup_configuration {
            enabled = true
        }

        ip_configuration {
            ipv4_enabled = true
            require_ssl = true
        }

        location_preference {
            zone = "us-central1-c"
        }

        maintenance_window {
            day = 7
            hour = 3
            update_track = "stable"
        }

        database_flags {
            name  = "maintenance_work_mem"
            value = "1048576"
        }
    }
}

resource "google_sql_database" "squadov-database" {
    name     = "squadov"
    instance = google_sql_database_instance.main-db.name
}

resource "google_sql_database" "squadov-update-database" {
    name     = "updates"
    instance = google_sql_database_instance.main-db.name
}

resource "google_sql_user" "default-user" {
    name     = var.postgres_user
    password = var.postgres_password
    instance = google_sql_database_instance.main-db.name
}

resource "google_sql_ssl_cert" "main-db-cert" {
    common_name = "main-db-cert"
    instance    = google_sql_database_instance.main-db.name
}

resource "google_sql_database_instance" "mysql-db" {
    name = var.mysql_instance_name
    database_version = "MYSQL_5_7"
    region = "us-central1"

    settings {
        tier = "db-custom-1-3840"
        availability_type = "ZONAL"

        backup_configuration {
            enabled = true
        }

        ip_configuration {
            ipv4_enabled = true
            require_ssl = true
        }

        location_preference {
            zone = "us-central1-c"
        }

        maintenance_window {
            day = 7
            hour = 3
            update_track = "stable"
        }
    }
}

resource "google_sql_database" "wordpress-database" {
    name     = "wordpress"
    instance = google_sql_database_instance.mysql-db.name
}

resource "google_sql_user" "mysql-user" {
    name     = var.mysql_user
    password = var.mysql_password
    instance = google_sql_database_instance.mysql-db.name
}

resource "google_sql_ssl_cert" "mysql-db-cert" {
    common_name = "mysql-db-cert"
    instance    = google_sql_database_instance.mysql-db.name
}