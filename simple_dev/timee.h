#include <WiFi.h>
#include <stdint.h>
#include <string.h>
#include "config.h"

// 1/1/2024 00:00
#define TIME_EPOCH_UNIX_MS 1704063600000
#define TIME_EPOCH_UNIX    1704063600

#define DAYS_IN_YEAR 365

static bool is_big_endian() {
    union {
        uint32_t i;
        char c[4];
    } e = { 0x01000000 };

    return e.c[0];
}

static void rev_B8(byte* dest, byte const* src) {
  dest[0] = src[7]; dest[1] = src[6];
  dest[2] = src[5]; dest[3] = src[4];
  dest[4] = src[3]; dest[5] = src[2];
  dest[6] = src[1]; dest[7] = src[0];
}

static uint64_t getTimeMS() {
  static uint64_t calibrate = 0;
  if (calibrate != 0) {
    return calibrate + millis();
  }

  WiFiClient client;
  uint64_t diff = millis();
  if (!client.connect(CLOCK_IP, CLOCK_PORT)) {
    return 0;
  }
  byte by[8];
  memset(by, 0, 8);
  while (client.available() < 8) { 
    delay(10); 
  }
  client.read(by, 8);
  diff = millis() - diff;
  if (is_big_endian()) {
    byte cpy[8];
    memcpy(cpy, by, 8);
    rev_B8(by, cpy);
  }
  diff >>= 1; // diff is now about the amount of ms from the time server to this
  calibrate = (*(uint64_t*) by) + diff;
  calibrate -= millis();
  client.stop();

  return getTimeMS();
}

struct Time {
  uint16_t ms;
  uint8_t  s;
  uint8_t  m;
  uint8_t  h;
  uint16_t DayInYear;
  uint16_t D;
  uint8_t  M;
  uint16_t Y;
};

#define TIME_DATE_STR_LEN 10
static void timeDateStr(char* out, Time t) {
  out[ 0] = ((t.M / 10)   % 10) + '0';
  out[ 1] = ((t.M)        % 10) + '0';
  out[ 2] = '/';
  out[ 3] = ((t.D / 10)   % 10) + '0';
  out[ 4] = ((t.D)        % 10) + '0';
  out[ 5] = '/';
  out[ 6] = ((t.Y / 1000) % 10) + '0';
  out[ 7] = ((t.Y / 100)  % 10) + '0';
  out[ 8] = ((t.Y / 10)   % 10) + '0';
  out[ 9] = ((t.Y)        % 10) + '0';
  out[10] = '\0';
}

#define TIME_HMS_STR_LEN 8
static void timeHMSStr(char* out, Time t) {
  out[0] = ((t.h / 10)   % 10) + '0';
  out[1] = ((t.h)        % 10) + '0';
  out[2] = ':';
  out[3] = ((t.m / 10)   % 10) + '0';
  out[4] = ((t.m)        % 10) + '0';
  out[5] = ':';
  out[6] = ((t.s / 10)   % 10) + '0';
  out[7] = ((t.s)        % 10) + '0';
  out[8] = '\0';
}

#define TIME_HMMS_STR_LEN (TIME_HMS_STR_LEN + 3)
static void timeHMMSStr(char* out, Time t) {
  timeHMSStr(out, t);
  out[TIME_HMS_STR_LEN+0] = ':';
  out[TIME_HMS_STR_LEN+1] = ((t.ms / 10) % 10) + '0';
  out[TIME_HMS_STR_LEN+2] = ((t.ms)      % 10) + '0';
  out[TIME_HMS_STR_LEN+3] = '\0';
}

#define TIME_STR_LEN (TIME_HMMS_STR_LEN + TIME_DATE_STR_LEN + 1)
static void timeStr(char* out, Time t) {
  timeDateStr(out, t);
  out[TIME_DATE_STR_LEN] = ' ';
  timeHMMSStr(&out[TIME_DATE_STR_LEN+1], t);
}

static Time timeFromMillis(uint64_t ti) {
  Time res;
  res.ms = ti % 1000;
  ti /= 1000;
  res.s = ti % 60;
  ti /= 60;
  res.m = ti % 60;
  ti /= 60;
  res.h = ti % 24;
  ti /= 24;

  // 2024, which is the time base is a leap year

  res.Y = 0;
  for (;;) {
    uint16_t year_n_days = DAYS_IN_YEAR;
    if (res.Y % 4 == 0) {
      year_n_days += 1;
    }

    res.DayInYear = ti % year_n_days;
    if (year_n_days > ti) {
      break;
    }
    ti -= year_n_days;

    res.Y ++;
  }

  uint8_t months[12] = { 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31 };
  if (res.Y % 4 == 0) {
    months[1] = 29;
  }
  
  uint16_t rem = res.DayInYear;
  for (res.M = 0; res.M < 12; res.M ++) {
    uint8_t d = months[res.M];
    if (d > rem) {
      break;
    }
    rem -= d;
  }
  res.D = rem + 1;
  res.M += 1;

  res.Y += 2024;
  return res;
};

static uint64_t unixToTimeMS(uint64_t unix) {
  unix -= TIME_EPOCH_UNIX;
  unix *= 1000;
  return unix;
}

static String timeCppString(Time t) {
  char buf[TIME_STR_LEN + 1];
  timeStr(buf, t);
  return buf;
}