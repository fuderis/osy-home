#include <IRremote.h>
#include <Servo.h>
#include <ArduinoJson.h>
#include "config.h"

#define RECV_PIN 2

const uint8_t relay_pins[4] = { 7, 8, 14, 15 };
const uint8_t servo_pins[4] = { A0, A1, A2, A3 };
const uint8_t motor_pins[2][2] = { {9,10}, {5,6} };

// Device states in the board’s memory:
bool relay_state[4] = {false, false, false, false};
bool servo_direction_toggle[4] = {false, false, false, false};
String motor_current_side[2] = {"left", "left"};

// Timers for background tasks (servo return, motor timers, hold).
struct RuntimeTask {
    bool active;
    unsigned long start_time;
    unsigned long duration;
    int target_value;
    uint8_t dev_type;
    uint8_t index;
    unsigned long last_packet_time;
};

// Array of tasks for background execution.
RuntimeTask runtime_tasks[4] = {0};

IRrecv irrecv(RECV_PIN);
decode_results results;
Servo servos[4];
StaticJsonDocument<384> doc;

void setup() {
  Serial.begin(115200);
  irrecv.enableIRIn();

  for (uint8_t i = 0; i < 4; i++) {
    pinMode(relay_pins[i], OUTPUT);
    servos[i].attach(servo_pins[i]);
    servos[i].write(0);
  }

  for (uint8_t i = 0; i < 2; i++) {
    pinMode(motor_pins[i][0], OUTPUT);
    pinMode(motor_pins[i][1], OUTPUT);
  }

  // PWM frequency setting
  TCCR1A = 0;
  TCCR1B = 0;
  TCCR1A = (1 << COM1A1) | (1 << COM1B1) | (1 << WGM11);
  TCCR1B = (1 << WGM13) | (1 << WGM12) | (1 << CS10);
  ICR1 = 799;
  OCR1A = 0;
  OCR1B = 0;
}

void loop() {
  handle_serial_input();
  handle_background_tasks();

  if (irrecv.decode(&results)) {
    handle_ir_code(results.value);
    send_code_to_pc(results.value);
    irrecv.resume();
  }
}

// Background timer handler (returning the servo to mid, stopping the motors, hold).
void handle_background_tasks() {
  unsigned long now = millis();

  for (uint8_t i = 0; i < 4; i++) {
    if (!runtime_tasks[i].active) continue;

    RuntimeTask& task = runtime_tasks[i];

    // 1. Servo drive returns to the original position (mid) 400 ms after the button is pressed.
    if (task.dev_type == 1) {
      if (now - task.start_time >= task.duration) {
        servos[task.index].write(task.target_value);
        send_servo_to_pc(task.index, task.target_value);
        task.active = false;
      }
    }
    // 2. Motor stops after the duration (timer or shutter movement).
    else if (task.dev_type == 2) {
      if (now - task.start_time >= task.duration) {
        stop_motor(task.index);
        task.active = false;
        send_motor_to_pc(task.index, 0, false, 0.0);
      }
    }
    // 3. Motor hold based on the timeout of the remote control packets.
    else if (task.dev_type == 3) {
      if (now - task.last_packet_time > 150) {
        stop_motor(task.index);
        task.active = false;
        send_motor_to_pc(task.index, 0, false, 0.0);
      }
    }
  }
}

void handle_serial_input() {
  if (Serial.available()) {
    String input = Serial.readStringUntil('\n');
    DeserializationError err = deserializeJson(doc, input);
    if (err) return;

    String type = doc["type"] | "";

    if (type == "metadata") {
      StaticJsonDocument<128> resp;
      resp["device"] = %UUID%;
      resp["version"] = %VERSION%;
      serializeJson(resp, Serial);
      Serial.println();
    }
    else if (type == "ping") {
      StaticJsonDocument<64> resp;
      resp["status"] = "pong";
      serializeJson(resp, Serial);
      Serial.println();
    }
    else if (type == "alias") {
      const char* name = doc["name"] | "";
      unsigned long pc_hold_ms = doc["hold"] | 0;
      execute_alias_by_name(name, pc_hold_ms);
    }
    else if (type == "relay") {
      int index = doc["index"] | 0;
      if (index >= 0 && index < 4) {
        bool enabled = doc["enable"] | false;
        relay_state[index] = enabled;
        digitalWrite(relay_pins[index], enabled ? HIGH : LOW);
        send_relay_to_pc(index, enabled);
      }
    }
    else if (type == "servo") {
      int index = doc["index"] | 0;
      if (index >= 0 && index < 4) {
        int target_deg = constrain(doc["degs"] | 0, 0, 180);
        servos[index].write(target_deg);
        send_servo_to_pc(index, target_deg);
      }
    }
    else if (type == "motor") {
      int index = doc["index"] | 0;
      if (index >= 0 && index < 2) {
        bool enable = doc["enable"] | false;
        String side = doc["side"] | "right";
        float power = doc["power"] | 0.0;

        if (!enable) {
          stop_motor(index);
          send_motor_to_pc(index, 0, false, 0.0);
        } else {
          start_motor(index, side, power);
          send_motor_to_pc(index, (side == "right" || side == "Clockwise") ? 1 : -1, true, power);
        }
      }
    }
  }
}

// Searches for and executes an alias.
void execute_alias_by_name(const char* name, unsigned long pc_hold_ms) {
  for (uint8_t i = 0; i < ALIAS_COUNT; i++) {
    if (strcmp(aliases[i].name, name) == 0) {
      trigger_device(aliases[i], pc_hold_ms);
      send_alias_to_pc(name);
      return;
    }
  }
}

// Processor of IR codes.
void handle_ir_code(unsigned long code) {
  static int last_matched_alias = -1;

  // processing service code for remote control repeat 0xFFFFFFFF (hold)
  if (code == 0xFFFFFFFF && last_matched_alias != -1) {
    AliasEntry alias = aliases[last_matched_alias];
    if (strcmp(alias.type, "motor") == 0 && alias.hold_enabled) {
      uint8_t m_idx = alias.index;
      // update timer for the last package in the hold mode
      for (uint8_t t = 0; t < 4; t++) {
        if (runtime_tasks[t].active && runtime_tasks[t].dev_type == 3 && runtime_tasks[t].index == alias.index) {
          runtime_tasks[t].last_packet_time = millis();
        }
      }
      return;
    }
  }

  for (uint8_t i = 0; i < ALIAS_COUNT; i++) {
    for (uint8_t j = 0; j < BIND_COUNT; j++) {
      if (aliases[i].codes[j] != 0 && code == aliases[i].codes[j]) {
        last_matched_alias = i;
        trigger_device(aliases[i], 0);
        send_alias_to_pc(aliases[i].name);
        return;
      }
    }
  }
}

// Action execution dispatcher based on the alias structure.
void trigger_device(const AliasEntry& alias, unsigned long pc_hold_ms) {
  if (strcmp(alias.type, "relay") == 0) {
    uint8_t r_idx = alias.index;
    relay_state[r_idx] = !relay_state[r_idx];
    digitalWrite(relay_pins[r_idx], relay_state[r_idx] ? HIGH : LOW);
    send_relay_to_pc(r_idx, relay_state[r_idx]);
  }
  else if (strcmp(alias.type, "servo") == 0) {
    uint8_t s_idx = alias.index;
    int target_deg;

    if (alias.mid_deg != -1) {
      servo_direction_toggle[s_idx] = !servo_direction_toggle[s_idx];
      target_deg = servo_direction_toggle[s_idx] ? alias.max_deg : alias.min_deg;

      servos[s_idx].write(target_deg);
      send_servo_to_pc(s_idx, target_deg);

      runtime_tasks[s_idx] = (RuntimeTask){
        .active = true,
        .start_time = millis(),
        .duration = 400,
        .target_value = alias.mid_deg,
        .dev_type = 1,
        .index = s_idx,
        .last_packet_time = 0
      };
    } else {
      servo_direction_toggle[s_idx] = !servo_direction_toggle[s_idx];
      target_deg = servo_direction_toggle[s_idx] ? alias.max_deg : alias.min_deg;
      servos[s_idx].write(target_deg);
      send_servo_to_pc(s_idx, target_deg);
    }
  }
  else if (strcmp(alias.type, "motor") == 0) {
    uint8_t m_idx = alias.index;
    String target_side = alias.side;

    if (strcmp(alias.side, "left-right") == 0) {
      motor_current_side[m_idx] = (motor_current_side[m_idx] == "right") ? "left" : "right";
      target_side = motor_current_side[m_idx];
    }

    unsigned long run_duration = alias.duration;
    if (pc_hold_ms > 0) {
      run_duration = pc_hold_ms;
    }

    start_motor(m_idx, target_side, alias.power);
    send_motor_to_pc(m_idx, target_side == "right" ? 1 : -1, true, alias.power);

    if (run_duration > 0) {
      runtime_tasks[m_idx] = (RuntimeTask){
        .active = true,
        .start_time = millis(),
        .duration = run_duration,
        .target_value = 0,
        .dev_type = 2,
        .index = m_idx,
        .last_packet_time = 0
      };
    } else if (alias.hold_enabled || pc_hold_ms > 0) {
      runtime_tasks[m_idx] = (RuntimeTask){
        .active = true,
        .start_time = millis(),
        .duration = 0,
        .target_value = 0,
        .dev_type = 3,
        .index = m_idx,
        .last_packet_time = millis()
      };
    }
  }
}

// --- Low‑level motor functions ---

void start_motor(uint8_t n, String side, float power) {
  power = constrain(power, 0.0f, 1.0f);
  bool is_right = (side == "right" || side == "Clockwise");

  if (n == 0) {
    uint16_t pwm = (uint16_t)(power * 799.0);
    if (is_right) { OCR1A = pwm; OCR1B = 0; }
    else { OCR1A = 0; OCR1B = pwm; }
  } else {
    uint8_t pwm = (uint8_t)(power * 255.0);
    if (is_right) {
      analogWrite(motor_pins[n][0], pwm);
      analogWrite(motor_pins[n][1], 0);
    } else {
      analogWrite(motor_pins[n][0], 0);
      analogWrite(motor_pins[n][1], pwm);
    }
  }
}

void stop_motor(uint8_t n) {
  if (n == 0) {
    OCR1A = 0;
    OCR1B = 0;
  } else {
    analogWrite(motor_pins[n][0], 0);
    analogWrite(motor_pins[n][1], 0);
  }
}

// --- Sending reports to a PC ---

void send_code_to_pc(unsigned long code) {
  StaticJsonDocument<64> doc;
  doc["type"] = "code";
  char out[18];
  sprintf(out, "0x%lX", code);
  doc["code"] = out;
  doc["millis"] = millis();
  serializeJson(doc, Serial);
  Serial.println();
}

void send_alias_to_pc(const char* name) {
  StaticJsonDocument<64> doc;
  doc["type"] = "alias";
  doc["name"] = name;
  serializeJson(doc, Serial);
  Serial.println();
}

void send_relay_to_pc(uint8_t index, bool st) {
  StaticJsonDocument<64> doc;
  doc["type"] = "relay";
  doc["index"] = index + 1;
  doc["enabled"] = st;
  serializeJson(doc, Serial);
  Serial.println();
}

void send_servo_to_pc(uint8_t index, int deg) {
  StaticJsonDocument<64> doc;
  doc["type"] = "servo";
  doc["index"] = index + 1;
  doc["degs"] = deg;
  serializeJson(doc, Serial);
  Serial.println();
}

void send_motor_to_pc(uint8_t index, int8_t dir, bool enabled, float power) {
  StaticJsonDocument<96> doc;
  doc["type"] = "motor";
  doc["index"] = index + 1;
  doc["direct"] = dir == -1 ? "left" : "right";
  doc["enabled"] = enabled;
  doc["power"] = power;
  serializeJson(doc, Serial);
  Serial.println();
}
