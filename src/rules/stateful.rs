//! Resource types that hold data, so destroying them can lose that data.
//!
//! Used by the `stateful-delete` and `stateful-replace` rules.
//!
//! # Adding a type
//!
//! Add a `(resource type, noun)` pair to [`STATEFUL`], keeping the list in
//! alphabetical order (a test checks this). The noun completes "... will be
//! destroyed", e.g. `"database"`. Check the type name exists in the
//! provider's docs or in `terraform providers schema -json` first; a
//! misspelled type silently never matches.
//!
//! Every type below was checked against the AWS provider 6.68 schema and the
//! Google and AzureRM providers' resource docs in October 2026.

/// Stateful resource types and what they hold, sorted by type.
const STATEFUL: &[(&str, &str)] = &[
    ("aws_backup_vault", "backup vault"),
    ("aws_db_instance", "database"),
    ("aws_docdb_cluster", "database cluster"),
    ("aws_dsql_cluster", "database cluster"),
    ("aws_dynamodb_table", "database table"),
    ("aws_ebs_volume", "disk volume"),
    ("aws_ecr_repository", "container image repository"),
    ("aws_efs_file_system", "file system"),
    ("aws_elasticache_cluster", "cache cluster"),
    ("aws_elasticache_replication_group", "cache cluster"),
    ("aws_elasticsearch_domain", "search cluster"),
    ("aws_fsx_lustre_file_system", "file system"),
    ("aws_fsx_ontap_file_system", "file system"),
    ("aws_fsx_openzfs_file_system", "file system"),
    ("aws_fsx_windows_file_system", "file system"),
    ("aws_glacier_vault", "archive vault"),
    ("aws_keyspaces_table", "database table"),
    ("aws_kinesis_stream", "data stream"),
    ("aws_kms_key", "encryption key"),
    ("aws_memorydb_cluster", "database cluster"),
    ("aws_msk_cluster", "Kafka cluster"),
    ("aws_neptune_cluster", "database cluster"),
    ("aws_opensearch_domain", "search cluster"),
    ("aws_rds_cluster", "database cluster"),
    ("aws_redshift_cluster", "data warehouse"),
    ("aws_s3_bucket", "storage bucket"),
    ("aws_s3_directory_bucket", "storage bucket"),
    ("aws_s3tables_table_bucket", "table bucket"),
    ("aws_secretsmanager_secret", "secret"),
    ("aws_timestreamwrite_table", "database table"),
    ("azurerm_cosmosdb_account", "database account"),
    ("azurerm_key_vault", "key vault"),
    ("azurerm_key_vault_key", "encryption key"),
    ("azurerm_key_vault_secret", "secret"),
    ("azurerm_managed_disk", "disk"),
    ("azurerm_mssql_database", "database"),
    ("azurerm_mssql_managed_instance", "database instance"),
    ("azurerm_mssql_server", "database server"),
    ("azurerm_mysql_flexible_server", "database server"),
    ("azurerm_postgresql_flexible_server", "database server"),
    ("azurerm_recovery_services_vault", "backup vault"),
    ("azurerm_redis_cache", "cache"),
    ("azurerm_storage_account", "storage account"),
    ("azurerm_storage_container", "storage container"),
    ("azurerm_storage_data_lake_gen2_filesystem", "file system"),
    ("azurerm_storage_share", "file share"),
    ("google_alloydb_cluster", "database cluster"),
    ("google_bigquery_dataset", "dataset"),
    ("google_bigquery_table", "table"),
    ("google_bigtable_instance", "database instance"),
    ("google_bigtable_table", "table"),
    ("google_compute_disk", "disk"),
    ("google_compute_region_disk", "disk"),
    ("google_filestore_instance", "file system"),
    ("google_firestore_database", "database"),
    ("google_kms_crypto_key", "encryption key"),
    ("google_memorystore_instance", "cache"),
    ("google_redis_instance", "cache"),
    ("google_secret_manager_secret", "secret"),
    ("google_spanner_database", "database"),
    ("google_spanner_instance", "database instance"),
    ("google_sql_database", "database"),
    ("google_sql_database_instance", "database instance"),
    ("google_storage_bucket", "storage bucket"),
];

/// What a stateful resource type holds, e.g. `"database"`, or `None` if the
/// type isn't in the list.
pub fn noun(resource_type: &str) -> Option<&'static str> {
    // The list is sorted, so a binary search finds the type quickly.
    STATEFUL
        .binary_search_by(|(ty, _)| (*ty).cmp(resource_type))
        .ok()
        .map(|i| STATEFUL[i].1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_is_sorted_without_duplicates() {
        // binary_search in `noun` depends on this.
        for pair in STATEFUL.windows(2) {
            assert!(
                pair[0].0 < pair[1].0,
                "{} must come before {}",
                pair[1].0,
                pair[0].0
            );
        }
    }

    #[test]
    fn finds_known_types_only() {
        assert_eq!(noun("aws_db_instance"), Some("database"));
        assert_eq!(noun("google_storage_bucket"), Some("storage bucket"));
        assert_eq!(noun("azurerm_managed_disk"), Some("disk"));
        assert_eq!(noun("aws_instance"), None);
        assert_eq!(noun(""), None);
    }
}
