#include <WiFi.h>
#include "json.h"
#include "config.h"
#include "timee.h"


static DS18B20 ds(26);

String generateUUID(uint64_t unix_time_ms, const uint8_t mac[6]) {
    // UUID fields
    uint64_t timestamp;
    uint16_t clock_seq;
    uint8_t node[6];

    // Copy the MAC address into the node
    memcpy(node, mac, 6);

    // Convert Unix time in milliseconds to UUID timestamp (100-nanosecond intervals since 1582-10-15)
    uint64_t uuid_epoch_offset = 12219292800000ULL; // Offset in milliseconds between UUID epoch and Unix epoch
    timestamp = (unix_time_ms + uuid_epoch_offset) * 10000; // Convert ms to 100-ns

    // Generate a random clock sequence
    srand((unsigned int)time(NULL));
    clock_seq = (uint16_t)(rand() & 0x3FFF); // 14 bits (0x3FFF)

    // Construct the UUID fields
    uint32_t time_low = (uint32_t)(timestamp & 0xFFFFFFFF);
    uint16_t time_mid = (uint16_t)((timestamp >> 32) & 0xFFFF);
    uint16_t time_hi_and_version = (uint16_t)((timestamp >> 48) & 0x0FFF);
    time_hi_and_version |= (1 << 12); // Set the version to 1

    uint8_t clock_seq_hi_and_reserved = (uint8_t)((clock_seq >> 8) & 0x3F);
    clock_seq_hi_and_reserved |= 0x80; // Set the two most significant bits to 10

    uint8_t clock_seq_low = (uint8_t)(clock_seq & 0xFF);

    // Format the UUID string
    char uuid_str[37];
    snprintf(uuid_str, 37,
             "%08x-%04x-%04x-%02x%02x-%02x%02x%02x%02x%02x%02x",
             time_low, time_mid, time_hi_and_version,
             clock_seq_hi_and_reserved, clock_seq_low,
             node[0], node[1], node[2], node[3], node[4], node[5]);

    return uuid_str;
}

String generateUUID() {
    byte mac[6];
    WiFi.macAddress(mac);
    uint64_t unix = UUID_UNIX_TIME;
    return generateUUID(unix, (uint8_t*) mac);
}

static uint64_t last_send_temp = 0;
static uint64_t last_keepalive;
static String uuid;

static WiFiClient client;

void setup() {
  Serial.begin(74880); // 74880
  Serial.println("\n\nHello world");
}

void loop() {
  if (WiFi.status() != WL_CONNECTED) {
    do {
      WiFi.begin(WLAN_SSID, WLAN_PW);
      Serial.println("Connecting to Wi-Fi...");
      delay(5000);
    } while (WiFi.status() != WL_CONNECTED);
    Serial.println("Connected to Wi-Fi");

    uuid = generateUUID();
    Serial.println("MAC: " + WiFi.macAddress());
    Serial.println("UUID address: " + uuid);

    for (int niter = 0; getTimeMS() == 0 && niter < 3; niter ++) {
      Serial.println("waiting for time server...");
      delay(1000);
    }
  }

  if (!client.connected()) {
    Serial.printf("Connecting to server %s:%d...\n", DEVCTRL_IP, DEVCTRL_PORT);
    while (!client.connect(DEVCTRL_IP, DEVCTRL_PORT)) {
      Serial.println("Connection to server failed, retry");
      delay(5000);
    }
  
    if (client.connected()) {
      Serial.println("Connected to server");
      // TODO: MAKE WORK FOR WHEN RTC_UNIX > 32 BIT
      client.printf("{ \"uuid\": \"%s\", \"rtc_unix\": %u, \"data\": { \"type\": \"Startup\", \"dev_type\": \"temp_v1\" } }\n",
        uuid.c_str(),
        (uint32_t) timeMSToUnix(getTimeMS()));
      last_keepalive = millis();
      client.flush();
    }
  }

  if (last_send_temp == 0 || millis() - last_send_temp > DATA_SEND_INTV) {
    while (ds.selectNext()) {
      uint8_t address[8];
      ds.getAddress(address);
      uint32_t addr0 = ((uint32_t*) address)[0];
      uint32_t addr1 = ((uint32_t*) address)[1];

      client.printf("{ \"uuid\": \"%s\", \"rtc_unix\": %u, \"data\": { \"type\": \"Transmit\", \"channel\": \"%u_%u\", \"data\": [%f] } }\n",
        uuid.c_str(),
        (uint32_t) timeMSToUnix(getTimeMS()),
        addr0, addr1,
        ds.getTempC());
      client.flush();
    }

    last_send_temp = millis();
  }

  if (millis() - last_keepalive > KEEPALIVE_INTV) {
    client.printf("{ \"uuid\": \"%s\", \"rtc_unix\": %u, \"data\": { \"type\": \"KeepAlive\" } }\n",
      uuid.c_str(),
      (uint32_t) timeMSToUnix(getTimeMS()));
    client.flush();

    last_keepalive = millis();
  }

  while (client.available()) {
    String serverMessage = client.readStringUntil('\n');
    Serial.println("Message from server: " + serverMessage);
  }

  delay(SLEEP_INTV);
} 
