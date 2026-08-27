# VigilShell Project Overview

## Purpose

VigilShell is a local-first professional server-management desktop application.
It gives a Human operator a complete operational workbench while offering
external Agents a separate, typed, and explicitly authorized operations lane.
It is not an unrestricted automation shell attached to a terminal.

## Product Form

- VigilShell is a per-user desktop product for macOS, Windows, and Linux.
- The initial distribution qualification order is macOS, Windows, then Linux;
  cross-platform boundaries remain required from the start.
- It stays resident after management windows close so trusted local supervision,
  MCP availability, notification routing, and operation settlement can continue.
- The Human GUI is opened on demand. Background residency does not make the App
  a remote control plane, system service, or unattended executor.

## Actors And Core Terms

- A **Human** operates connections, terminals, files, monitoring, services, and
  controlled changes through the Human Lane.
- An **Agent** operates only through a short-lived Agent Session that is bound to
  an explicit Agent Scope and typed capabilities.
- A **Managed Server** is a stable Human-facing server identity that can own one
  or more Connection Profiles.
- A **Connection Profile** owns endpoint routing and authentication mechanics. It
  is not an authorization grant for an Agent.
- A **Project** is a Human-facing grouping of Managed Servers. Connection
  Profiles inherit that navigation placement, which has no Agent authorization
  meaning.
- An **Agent Scope** is a reusable local authorization boundary for resources,
  capabilities, and policy constraints.

## Durable Scope

VigilShell's durable product scope includes professional SSH operations:
connection management, terminal work, SFTP, transfers, tunnels, server and
service inspection, logs, and controlled server changes. The Agent Lane is
defined for bounded diagnosis and operations through capability-specific
contracts rather than through a raw session.

## Non-Goals

- A cloud-hosted, multi-tenant, or team-control-plane product.
- Raw SSH access, terminal access, arbitrary shell execution, arbitrary file
  access, raw endpoints, or credential access for Agents.
- A separate database IDE; databases, caches, and queues are managed as services
  on a server.
- Unattended write automation that bypasses Scope, policy, risk, confirmation,
  redaction, and audit controls.
