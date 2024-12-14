CREATE DATABASE IF NOT EXISTS devices;

/**
  stores the current active devices.
  uses memory engine because devices will reconnect if server crashes.
  devices that weren't active for 10 seconds will be automatically removed by ClickHouse*/
CREATE TABLE IF NOT EXISTS devices.active (
    uuid UUID,
    last_alive DateTime TTL last_alive + INTERVAL 10 SECOND,
    type String
) ENGINE = Memory()
    PRIMARY KEY (uuid)
    ORDER BY (uuid);

