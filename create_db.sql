CREATE DATABASE IF NOT EXISTS devices;

/**
  stores the current active devices.
  devices that weren't active for 10 seconds will be automatically removed by ClickHouse*/
CREATE TABLE IF NOT EXISTS devices.active (
    uuid UUID PRIMARY KEY,
    last_alive DateTime,
    type String
) ENGINE = MergeTree()
TTL last_alive + INTERVAL 10 SECOND DELETE;
