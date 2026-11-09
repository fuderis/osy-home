<p align="center">
  <img src="https://raw.githubusercontent.com/fuderis/osy-kernel/main/assets/logo.png" alt="Logo" width="80" />
</p>

<h1 align="center">Osy Home Agent</h1>
<p align="center">
  <strong>Smart home agent for Osy Kernel</strong><br>
</p>

## Overview

**Osy Home** is a specialized modular agent for the **[Osy Kernel](https://github.com/fuderis/osy-kernel)** ecosystem, providing smart home management (Arduino Pro Micro + IR Remote) over lightning-fast IPC.

---

## Dependencies

Osy agents are structured within the kernel's Cargo workspace.

This design shares foundational core crates (`rigging`, `pearce`, `cistern`, `anylm`, etc.) across all services,
ensuring consistent types, optimal binary size, and unified dependencies while keeping agents in independent repositories.

---

## Installation

1. Clone agent repository into the `agents/` directory in the root of your local **Osy Kernel** source tree:

```bash
cd .../osy/agents
git clone https://github.com/fuderis/osy-home.git home
```

2. Navigate back to root and run the installation command:

```bash
cd ..
make install-agent name=home
```

or install all agents located in the `agents/` folder at once:

```bash
make install-agents
```

---

## Licensing

This project is distributed under the **[GNU General Public License v3.0](LICENSE.md)**.

### Dual Licensing

* **Open Source Use:** Free to use, modify, and deploy in non-commercial or open-source projects under GPL-3.0.
* **Commercial License:** Required to integrate into proprietary commercial products without source code disclosure.

> Contact: **Bulat Sharipov** ([@fuderis](https://github.com/fuderis) / `synapdrake@ya.ru`).

