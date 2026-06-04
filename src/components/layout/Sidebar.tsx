import { SessionList } from "../sessions/SessionList";

export function Sidebar() {
  return (
    <div className="flex h-full flex-col bg-gray-50 dark:bg-gray-900">
      <SessionList />
    </div>
  );
}
