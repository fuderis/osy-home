#pragma once
#include <Arduino.h>

#define BIND_COUNT %BIND_COUNT%

struct AliasEntry {
    const char* name;
    const char* type;      // "relay", "servo" or "motor"
    uint8_t index;         // 0..3 for relay/servo, 0..1 for motors
    unsigned long codes[BIND_COUNT];
    
    // Fields for servo
    int min_deg;
    int max_deg;
    int mid_deg;           // -1 if not used

    // Fields for motors
    const char* side;      // "left", "right", "left-right"
    float power;           // 0.0 - 1.0
    unsigned long duration;// Work duration in ms
    bool hold_enabled;     // Hold mode
};

#define ALIAS_COUNT %ALIAS_COUNT%

const AliasEntry aliases[ALIAS_COUNT] = {
  %ALIASES%
};
