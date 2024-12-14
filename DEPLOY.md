## Step 1: Database
(On Windows, you need to do this in WSL)

On all other operating systems: `curl https://clickhouse.com/ | sh` and follow the steps.

Now create a directory for the database data and `cd` into it.
Next, you need to start the server. You can either use `clickhouse start` to run it in the background, or you can run `clickhouse-server`.

Now, you need to run `clickhouse-client create_db.sql` to create the DB structure.

## Step 2: devctrl
`cd` into `devctrl`, copy the `config.default.json` to `config.json` and modify it,
and start the device control service by doing `cargo run`.

## Step 3: IOT Devices
`cd` into `simple_dev`, copy the `config.default.h` to `config.h` and modify it.
Now open it in the Arduino IDE and flash your ESP32