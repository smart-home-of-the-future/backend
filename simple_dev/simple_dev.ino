#include <WiFi.h>
#include "json.h"
#include "config.h"
#include "timee.h"

// TODO: FIX THIS STUPID THING
String generateUUID() {
    // Step 1: Get the current time in milliseconds (use millis or micros)
    unsigned long long timestamp = millis();  // Using millis for simplicity (you can use micros for more precision)

    // Step 2: Get the MAC address of the device (48-bit node)
    byte mac[6];
    WiFi.macAddress(mac);

    // Step 3: Prepare parts for UUID v1
    // Time low (32 bits)
    unsigned long time_low = timestamp & 0xFFFFFFFF;

    // Time mid (16 bits)
    unsigned long time_mid = (timestamp >> 32) & 0xFFFF;

    // Time hi and version (16 bits): Version 1 is represented as the upper 4 bits
    unsigned long time_hi_and_version = ((timestamp >> 48) & 0x0FFF) | 0x1000;  // 0x1000 sets the version to 1

    // Step 4: Build the UUID
    String uuid = "";

    // Add time_hi_and_version
    uuid += String(time_hi_and_version, HEX);
    uuid += "-";

    // Add time_mid
    uuid += String(time_mid, HEX);
    uuid += "-";

    // Add time_low
    uuid += String(time_low, HEX);
    uuid += "-";

    // Add clock sequence (16 bits), we use 0 for simplicity
    uuid += "0000";  // 4 hex characters (16 bits)

    uuid += "-";

    // Add node (MAC address)
    for (int i = 0; i < 6; i++) {
        if (mac[i] < 16) {
            uuid += "0";
        }
        uuid += String(mac[i], HEX);
    }

    return uuid;
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

  Serial.println(getTimeMS());
  Serial.println(timeCppString(timeFromMillis(getTimeMS())));

  // Connect to the server
  Serial.printf("Connecting to server %s:%d...\n", DEVCTRL_IP, DEVCTRL_PORT);
  while (!client.connect(DEVCTRL_IP, DEVCTRL_PORT)) {
    Serial.println("Connection to server failed");
    delay(1000);
  }
  Serial.println("Connected to server");
 
  // send startup message
  client.printf("{ \"uuid\": \"%s\", \"data\": { \"type\": \"Startup\", \"dev_type\": \"movement_v1\" } }\n", uuid.c_str());
  client.flush();
}

void loop(){
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
