#include <WiFi.h>
#include "json.h"
#include "config.h"
#include "timee.h"

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
    uint64_t unix = timeMSToUnix(getTimeMS());
    return generateUUID(unix, (uint8_t*) mac);
}

WiFiClient client;
void setup() {
  Serial.begin(9600);
  pinMode(LED_PIN, OUTPUT);
  pinMode(PIR_PIN, INPUT);

  // Connect to Wi-Fi
  Serial.println("Connecting to Wi-Fi...");
  WiFi.begin(WLAN_SSID, WLAN_PW);
  while (WiFi.status() != WL_CONNECTED) {
    delay(1000);
    Serial.println("Connecting...");
  }
  Serial.println("Connected to Wi-Fi");

  String uuid = generateUUID();
  Serial.println("MAC: " + WiFi.macAddress());
  Serial.println("UUID address: " + uuid);

  while (getTimeMS() == 0) {
    Serial.println("Could not get time!");
    delay(1000);
  }

  Serial.println(timeCppString(timeFromMillis(getTimeMS())));

  // Connect to the server
  Serial.printf("Connecting to server %s:%d...\n", DEVCTRL_IP, DEVCTRL_PORT);
  while (!client.connect(DEVCTRL_IP, DEVCTRL_PORT)) {
    Serial.println("Connection to server failed");
    delay(1000);
  }
  Serial.println("Connected to server");
 
  // send startup message
  // TODO: MAKE WORK FOR WHEN RTC_UNIX > 32 BIT
  client.printf("{ \"uuid\": \"%s\", \"rtc_unix\": %u, \"data\": { \"type\": \"Startup\", \"dev_type\": \"movement_v1\" } }\n",
    (uint32_t) timeMSToUnix(getTimeMS()),
    uuid.c_str());
  client.flush();
}

void loop() {
  int pirStat = digitalRead(PIR_PIN);
  if (pirStat == HIGH) {
    digitalWrite(LED_PIN, HIGH);
    //Serial.println("motion!");
  } 
  else {
    digitalWrite(LED_PIN, LOW); // turn LED OFF if we have no motion
  }
  
  while (client.available()) {
    String serverMessage = client.readStringUntil('\n');
    Serial.println("Message from server: " + serverMessage);
  }
} 
