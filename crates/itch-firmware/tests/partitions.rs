use itch_firmware::settings::PARTITION;

/// `(offset, size)` of the partition `name` in `partitions.csv`.
fn partition(name: &str) -> (u32, u32) {
    let table = include_str!("../partitions.csv");
    let row = table
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.split(',').map(str::trim).collect::<Vec<_>>())
        .find(|row| row[0] == name)
        .unwrap_or_else(|| panic!("no {name} partition"));
    let number = |field: &str| u32::from_str_radix(field.trim_start_matches("0x"), 16).unwrap();
    (number(row[3]), number(row[4]))
}

#[test]
fn the_settings_are_where_the_partition_table_puts_them() {
    let (offset, size) = partition("settings");
    assert_eq!(PARTITION, offset..offset + size);
}

#[test]
fn the_settings_partition_comes_after_the_whole_app_partition() {
    let (app, app_size) = partition("factory");
    assert!(PARTITION.start >= app + app_size);
}
