import { useUIStore } from "../../stores/uiStore";
import { Sidebar } from "./Sidebar";
import { ChatView } from "../chat/ChatView";
import { Header } from "./Header";
import { StatusBar } from "./StatusBar";

export function AppShell() {
  const { sidebarOpen, darkMode } = useUIStore();

  return (
    <div className={`flex h-full flex-col ${darkMode ? "dark" : ""}`}>
      <Header />
      <div className="flex flex-1 overflow-hidden">
        {sidebarOpen && (
          <div className="w-[280px] flex-shrink-0 border-r border-gray-200 dark:border-gray-700">
            <Sidebar />
          </div>
        )}
        <div className="flex-1 overflow-hidden">
          <ChatView />
        </div>
      </div>
      <StatusBar />
    </div>
  );
}
