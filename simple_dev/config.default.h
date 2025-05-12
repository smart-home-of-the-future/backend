#ifndef _CONFIG_H
#define _CONFIG_H

#define WLAN_SSID "awesome home network"
#define WLAN_PW   "1234"

#define DEVCTRL_IP   "LOCAL IP HERE"
#define DEVCTRL_PORT 1234

#define CLOCK_IP    "LOCAL IP HERE"
#define CLOCK_PORT  4321

#define DATA_SEND_INTV 5000
#define KEEPALIVE_INTV 5000  // note that most data sends also count as keepalive

#define SLEEP_INTV 2000

// used together with MAC to form UUID
// doesn't really need to be changed on different devices
#define UUID_UNIX_TIME 1747076644

#endif