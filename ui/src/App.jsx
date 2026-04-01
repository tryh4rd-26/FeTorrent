import { useEffect, useMemo, useState } from 'react';
import {
  Activity,
  AlertCircle,
  CheckCircle2,
  Clock,
  Download,
  HardDrive,
  Menu,
  Network,
  Pause,
  Play,
  Plus,
  Settings,
  Trash2,
  Upload,
  X,
} from 'lucide-react';
import { useFeTorrentStream, pauseTorrent, resumeTorrent, removeTorrent, addTorrent, fetchSettings, updateSettings } from './api';
import { formatBytes } from './lib/utils';

// ShadCN Components
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { 
  Dialog, 
  DialogContent, 
  DialogDescription, 
  DialogHeader, 
  DialogTitle, 
  DialogTrigger,
  DialogFooter
} from "@/components/ui/dialog";
import { Toaster } from "@/components/ui/sonner";
import { toast } from "sonner";
import { cn } from "@/lib/utils";

export default function App() {
  const { torrents, globalStats, connected } = useFeTorrentStream();
  const [addingMagnet, setAddingMagnet] = useState("");
  const [activeTab, setActiveTab] = useState("transfer");
  const [filter, setFilter] = useState("All Torrents");
  const [isAddOpen, setIsAddOpen] = useState(false);
  const [settings, setSettings] = useState(null);
  const [sidebarOpen, setSidebarOpen] = useState(true);

  useEffect(() => {
    fetchSettings().then(setSettings).catch(console.error);
  }, []);

  const handleAdd = async (e) => {
    e.preventDefault();
    if (!addingMagnet.trim()) return;
    
    const fd = new FormData();
    fd.append('magnet', addingMagnet);
    
    toast.promise(addTorrent(fd), {
       loading: 'Initializing swarm...',
       success: 'Torrent added',
       error: 'Failed to add torrent',
    });

    setAddingMagnet("");
    setIsAddOpen(false);
  };

  const filteredTorrents = torrents.filter(t => {
    if (filter === "All Torrents") return true;
    if (filter === "Downloading") return t.status.toLowerCase().includes("downloading");
    if (filter === "Completed") return t.progress >= 1;
    if (filter === "Active") return t.dl_speed > 0 || t.ul_speed > 0;
    if (filter === "Inactive") return t.dl_speed === 0 && t.ul_speed === 0;
    return true;
  });

  const activeCount = useMemo(
    () => torrents.filter(t => t.dl_speed > 0 || t.ul_speed > 0).length,
    [torrents]
  );

  const completedCount = useMemo(
    () => torrents.filter(t => t.progress >= 1).length,
    [torrents]
  );

  const downloadingCount = useMemo(
    () => torrents.filter(t => t.status.toLowerCase().includes('download')).length,
    [torrents]
  );

  const statItems = [
    {
      label: 'Download',
      value: `${formatBytes(globalStats.dl_speed)}/s`,
      icon: Download,
      color: 'orange',
    },
    {
      label: 'Upload',
      value: `${formatBytes(globalStats.ul_speed)}/s`,
      icon: Upload,
      color: 'green',
    },
    {
      label: 'Active Torrents',
      value: String(globalStats.active_torrents),
      icon: Network,
      color: 'blue',
    },
    {
      label: 'Queued Size',
      value: formatBytes(torrents.reduce((sum, t) => sum + (t.total_size || 0), 0)),
      icon: HardDrive,
      color: 'slate',
    },
  ];

  return (
    <div className="dark min-h-screen w-full bg-background text-foreground font-sans antialiased">
      <Toaster position="top-right" theme="dark" />

      <div className="flex h-screen">
        {/* Overlay for mobile */}
        {sidebarOpen && (
          <div 
            className="fixed inset-0 z-30 bg-black/50 lg:hidden transition-opacity"
            onClick={() => setSidebarOpen(false)}
          />
        )}

        {/* Sidebar */}
        <aside
          className={cn(
            "fixed lg:static z-40 h-screen w-64 border-r border-border bg-gradient-to-b from-card to-card/50 overflow-y-auto transition-all duration-300 ease-out",
            sidebarOpen ? "translate-x-0" : "-translate-x-full lg:translate-x-0"
          )}
        >
          <div className="p-5 space-y-6">
            {/* Header */}
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-3">
                <div className="flex size-10 items-center justify-center rounded-lg bg-gradient-to-br from-orange-500 to-orange-600 shadow-lg shadow-orange-500/20">
                  <Activity className="size-5 text-white" />
                </div>
                <div>
                  <p className="font-bold text-sm leading-tight">FeTorrent</p>
                  <p className="text-xs text-muted-foreground">Control Panel</p>
                </div>
              </div>
              <button
                onClick={() => setSidebarOpen(false)}
                className="lg:hidden p-1 hover:bg-muted rounded transition-colors"
              >
                <X className="size-4" />
              </button>
            </div>

            {/* Navigation */}
            <nav className="space-y-2">
              <SideNavItem
                label="Transfer"
                active={activeTab === 'transfer'}
                onClick={() => {
                  setActiveTab('transfer');
                  setSidebarOpen(false);
                }}
              />
              <SideNavItem
                label="Settings"
                active={activeTab === 'settings'}
                onClick={() => {
                  setActiveTab('settings');
                  setSidebarOpen(false);
                }}
              />
            </nav>

            {/* Stats Box */}
            <div className="rounded-xl border border-border/60 bg-muted/30 p-4">
              <p className="text-xs font-semibold text-muted-foreground mb-3 uppercase tracking-wider">Queue</p>
              <div className="space-y-2 text-sm">
                <div className="flex justify-between">
                  <span className="text-muted-foreground">All</span>
                  <span className="font-semibold text-orange-400">{torrents.length}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-muted-foreground">Downloading</span>
                  <span className="font-semibold text-orange-400">{downloadingCount}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-muted-foreground">Active</span>
                  <span className="font-semibold text-green-400">{activeCount}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-muted-foreground">Completed</span>
                  <span className="font-semibold text-blue-400">{completedCount}</span>
                </div>
              </div>
            </div>

            {/* Connection Status */}
            <div className={cn(
              "rounded-lg px-3 py-2 text-xs font-medium flex items-center gap-2 transition-all",
              connected 
                ? "border border-green-500/30 bg-green-500/10 text-green-400" 
                : "border border-red-500/30 bg-red-500/10 text-red-400"
            )}>
              <div className={cn(
                "size-2 rounded-full animate-pulse",
                connected ? "bg-green-500" : "bg-red-500"
              )} />
              {connected ? 'Connected' : 'Disconnected'}
            </div>
          </div>
        </aside>

        {/* Main Content */}
        <main className="flex-1 flex flex-col overflow-hidden">
          {/* Header */}
          <header className="border-b border-border/50 bg-card/40 backdrop-blur px-6 py-4 flex items-center justify-between">
            <div className="flex items-center gap-4">
              <button
                onClick={() => setSidebarOpen(!sidebarOpen)}
                className="lg:hidden p-2 hover:bg-muted rounded-lg transition-colors"
              >
                <Menu className="size-5" />
              </button>
              <div>
                <h1 className="text-xl font-bold">Torrent Control</h1>
                <p className="text-xs text-muted-foreground">Manage your downloads</p>
              </div>
            </div>

            <div className="flex items-center gap-3 flex-wrap justify-end">
              <FilterButton label="All Torrents" active={filter === 'All Torrents'} onClick={setFilter} />
              <FilterButton label="Downloading" active={filter === 'Downloading'} onClick={setFilter} />
              <FilterButton label="Completed" active={filter === 'Completed'} onClick={setFilter} />

              <Dialog open={isAddOpen} onOpenChange={setIsAddOpen}>
                <DialogTrigger asChild>
                  <Button className="gap-2 bg-gradient-to-r from-orange-500 to-orange-600 hover:from-orange-600 hover:to-orange-700 shadow-lg shadow-orange-500/20">
                    <Plus className="size-4" />
                    Add Torrent
                  </Button>
                </DialogTrigger>
                <DialogContent className="border-border/60 bg-card/95 backdrop-blur">
                  <DialogHeader>
                    <DialogTitle>Add Torrent</DialogTitle>
                    <DialogDescription>Paste a magnet URI to start downloading.</DialogDescription>
                  </DialogHeader>
                  <form onSubmit={handleAdd} className="space-y-4">
                    <Input
                      placeholder="magnet:?xt=urn:btih:..."
                      value={addingMagnet}
                      onChange={e => setAddingMagnet(e.target.value)}
                      autoFocus
                    />
                    <DialogFooter>
                      <Button type="submit" className="w-full bg-gradient-to-r from-orange-500 to-orange-600">
                        Add to Queue
                      </Button>
                    </DialogFooter>
                  </form>
                </DialogContent>
              </Dialog>
            </div>
          </header>

          {/* Content */}
          <div className="flex-1 overflow-y-auto">
            <div className="p-6 space-y-6">
              {activeTab === 'settings' ? (
                <SettingsView settings={settings} onSave={updateSettings} />
              ) : (
                <>
                  {/* Stats Grid */}
                  <section className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
                    {statItems.map(({ label, value, icon: Icon, color }) => (
                      <Card key={label} className={cn(
                        "border border-border/60 bg-gradient-to-br from-card to-card/50 overflow-hidden group hover:border-orange-500/30 transition-all duration-300",
                        color && `hover:shadow-lg hover:shadow-${color}/20`
                      )}>
                        <CardContent className="p-5">
                          <div className="flex items-start justify-between mb-3">
                            <p className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">{label}</p>
                            <div className="p-2 rounded-lg bg-muted/50 group-hover:bg-orange-500/10 transition-colors">
                              <Icon className="size-4 text-orange-400" />
                            </div>
                          </div>
                          <p className="text-2xl font-bold tracking-tight">{value}</p>
                        </CardContent>
                      </Card>
                    ))}
                  </section>

                  {/* Torrents Table */}
                  <section className="overflow-hidden rounded-lg border border-border/60 bg-card/50 backdrop-blur">
                    <div className="grid grid-cols-[2fr_1fr_1fr_1fr_1fr_1fr_130px] gap-3 border-b border-border/40 px-5 py-4 bg-muted/20 text-xs font-semibold text-muted-foreground uppercase tracking-wider">
                      <span>Name</span>
                      <span>Progress</span>
                      <span>Download</span>
                      <span>Upload</span>
                      <span>Peers</span>
                      <span>Status</span>
                      <span className="text-right">Actions</span>
                    </div>

                    <div className="divide-y divide-border/30">
                      {filteredTorrents.length === 0 ? (
                        <div className="px-5 py-12 text-center">
                          <p className="text-muted-foreground text-sm">No torrents in this view</p>
                        </div>
                      ) : (
                        filteredTorrents.map((torrent, i) => (
                          <TorrentRow 
                            key={torrent.id} 
                            torrent={torrent} 
                            alternate={i % 2 === 1}
                          />
                        ))
                      )}
                    </div>
                  </section>
                </>
              )}
            </div>
          </div>
        </main>
      </div>

      {!connected && (
        <div className="fixed bottom-4 right-4 flex items-center gap-3 rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-3 text-sm text-red-400 animate-pulse shadow-lg shadow-red-500/20">
          <AlertCircle className="size-5 shrink-0" />
          <span>Daemon disconnected</span>
        </div>
      )}
    </div>
  );
}

function SideNavItem({ label, active, onClick }) {
  return (
    <button 
      onClick={onClick}
      className={cn(
        "w-full rounded-md border px-3 py-2 text-left text-sm transition-colors",
        active 
          ? "border-primary/40 bg-primary/10 text-foreground" 
          : "border-transparent text-muted-foreground hover:border-border/80 hover:bg-muted/50 hover:text-foreground"
      )}
    >
      {label}
    </button>
  );
}

function FilterButton({ label, active, onClick }) {
  return (
    <Button
      variant={active ? 'secondary' : 'ghost'}
      size="sm"
      className={cn('text-xs', active && 'ring-1 ring-border')}
      onClick={() => onClick(label)}
    >
      {label}
    </Button>
  );
}


function formatETA(seconds) {
  if (seconds === null || seconds === undefined) return '--';
  if (seconds === 0) return '0s';
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}

function TorrentRow({ torrent, alternate }) {
  const { id, name, downloaded, total_size, dl_speed, ul_speed, progress, status, num_seeds, num_leechers, eta_secs } = torrent;
  const lower = status.toLowerCase();
  const isDownloading = lower.includes('downloading') && progress < 1;
  const isSeeding = progress >= 1;
  const isPaused = lower.includes('paused');
  const isError = lower.includes('error');
  const isMetadata = lower.includes('metadata');

  let statusClass = 'status-paused';
  let statusLabel = status;
  let statusIcon = null;

  if (isError) {
    statusClass = 'status-error';
    statusLabel = 'Error';
    statusIcon = AlertCircle;
  } else if (isSeeding) {
    statusClass = 'status-seeding';
    statusLabel = 'Seeding';
    statusIcon = CheckCircle2;
  } else if (isDownloading) {
    statusClass = 'status-downloading';
    statusLabel = 'Downloading';
    statusIcon = Download;
  } else if (isPaused) {
    statusClass = 'status-paused';
    statusLabel = 'Paused';
    statusIcon = Pause;
  } else if (isMetadata) {
    statusClass = 'status-metadata';
    statusLabel = 'Metadata';
    statusIcon = Clock;
  }

  const StatusIcon = statusIcon;

  return (
    <div className={cn(
      "grid grid-cols-[2fr_1fr_1fr_1fr_1fr_1fr_130px] gap-3 items-center px-5 py-4 transition-all duration-200 hover:bg-muted/30",
      alternate && "bg-muted/15"
    )}>
      <div className="min-w-0">
        <p className="truncate font-medium text-sm" title={name}>{name}</p>
        <p className="text-xs text-muted-foreground">
          {formatBytes(downloaded)} / {formatBytes(total_size)}
        </p>
      </div>

      <div className="space-y-1">
        <div className="flex justify-between items-center mb-1">
          <span className="text-xs font-semibold text-orange-400">{(progress * 100).toFixed(1)}%</span>
        </div>
        <div className="h-1.5 bg-muted rounded-full overflow-hidden">
          <div 
            className="h-full rounded-full bg-gradient-to-r from-orange-500 to-orange-400 transition-all duration-300"
            style={{ width: `${(progress * 100).toFixed(2)}%` }} 
          />
        </div>
      </div>

      <div className="text-xs">
        <span className="badge-download font-medium">{formatBytes(dl_speed)}</span>
        <span className="text-muted-foreground">/s</span>
      </div>

      <div className="text-xs">
        <span className="badge-upload font-medium">{formatBytes(ul_speed)}</span>
        <span className="text-muted-foreground">/s</span>
      </div>

      <div className="text-xs text-muted-foreground text-center">
        {num_seeds}↑ {num_leechers}↓
      </div>

      <div>
        <Badge className={cn("capitalize border text-xs", statusClass)}>
          {StatusIcon && <StatusIcon className="mr-1 size-3" />}
          {statusLabel}
        </Badge>
      </div>

      <div className="flex justify-end gap-1">
        <Button 
          size="sm" 
          variant="ghost" 
          onClick={() => isPaused ? resumeTorrent(id) : pauseTorrent(id)}
          className="hover:bg-orange-500/10 hover:text-orange-400 transition-colors h-8 w-8 p-0"
          title={isPaused ? "Resume" : "Pause"}
        >
          {isPaused ? <Play className="size-4" /> : <Pause className="size-4" />}
        </Button>
        <Button 
          size="sm" 
          variant="ghost" 
          onClick={() => removeTorrent(id)}
          className="hover:bg-red-500/10 hover:text-red-400 transition-colors h-8 w-8 p-0"
          title="Remove"
        >
          <Trash2 className="size-4" />
        </Button>
      </div>
    </div>
  );
}

function SettingsView({ settings, onSave }) {
  const [formData, setFormData] = useState(settings || {
    server: { port: 6977, bind: "127.0.0.1" },
    downloads: { directory: "", max_peers: 200 },
    limits: { download_kbps: 0, upload_kbps: 0 }
  });

  if (!settings) return <div className="text-center py-20 text-muted-foreground">Loading settings...</div>;

  const handleSave = () => {
    onSave(formData);
    toast.success("Settings saved successfully");
  };

  return (
    <Card className="max-w-4xl border border-border/70 bg-card/50">
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <Settings className="size-4" />
          Settings
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="grid gap-4 md:grid-cols-2">
          <div className="space-y-2 md:col-span-2">
            <label className="text-xs text-muted-foreground">Download Directory</label>
            <Input
              value={formData.downloads.directory}
              onChange={e => setFormData({ ...formData, downloads: { ...formData.downloads, directory: e.target.value } })}
            />
          </div>

          <div className="space-y-2">
            <label className="text-xs text-muted-foreground">Server Port</label>
            <Input
              type="number"
              value={formData.server.port}
              onChange={e => setFormData({ ...formData, server: { ...formData.server, port: Number(e.target.value) || 0 } })}
            />
          </div>

          <div className="space-y-2">
            <label className="text-xs text-muted-foreground">Max Peers</label>
            <Input
              type="number"
              value={formData.downloads.max_peers}
              onChange={e => setFormData({ ...formData, downloads: { ...formData.downloads, max_peers: Number(e.target.value) || 0 } })}
            />
          </div>

          <div className="space-y-2">
            <label className="text-xs text-muted-foreground">Download Limit (KB/s)</label>
            <Input
              type="number"
              value={formData.limits.download_kbps}
              onChange={e => setFormData({ ...formData, limits: { ...formData.limits, download_kbps: Number(e.target.value) || 0 } })}
            />
          </div>

          <div className="space-y-2">
            <label className="text-xs text-muted-foreground">Upload Limit (KB/s)</label>
            <Input
              type="number"
              value={formData.limits.upload_kbps}
              onChange={e => setFormData({ ...formData, limits: { ...formData.limits, upload_kbps: Number(e.target.value) || 0 } })}
            />
          </div>
        </div>

        <div className="flex gap-2">
          <Button onClick={handleSave}>Save</Button>
          <Button variant="outline" onClick={() => setFormData(settings)}>
            Reset
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
