import type { AppInstance } from "../../../types/generated/AppInstance";
import type { DesktopApp } from "../../../types/generated/DesktopApp";
import type { DesktopAppId } from "../../../types/generated/DesktopAppId";

export interface InstanceCommands {
  list_desktop_apps: { args: Record<string, never>; result: DesktopApp[] };
  list_app_instances: { args: { app: DesktopAppId }; result: AppInstance[] };
  create_app_instance: { args: { app: DesktopAppId; name: string }; result: AppInstance };
  start_app_instance: { args: { id: string }; result: AppInstance };
  stop_app_instance: { args: { id: string }; result: AppInstance };
  delete_app_instance: { args: { id: string }; result: void };
}

export type { AppInstance, DesktopApp, DesktopAppId };
