import { useEffect, useMemo, useState } from 'react';
import {
  Activity,
  CheckCircle2,
  Clock,
  Download,
  FileText,
  FolderOpen,
  HardDrive,
  Menu,
  Moon,
  MoreHorizontal,
  Pause,
  Play,
  Plus,
  Search,
  Settings,
  Sun,
  Trash2,
  Upload,
  Users,
  X,
} from 'lucide-react';
import {
  addTorrent,
  fetchSettings,
  pauseTorrent,
  removeTorrent,
  resumeTorrent,
  selectDirectory,
  updateSettings,
  useFeTorrentStream,
} from './api';
import { formatBytes } from './lib/utils';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Toaster } from '@/components/ui/sonner';
import { cn } from '@/lib/utils';
import { toast } from 'sonner';

function initialTheme() {
  const value = localStorage.getItem('fetorrent-theme');
  if (value === 'light' || value === 'dark') return value;
  return 'dark';
}

const magnetInputClass =
  'min-h-64 w-full rounded-xl border border-input bg-background px-3 py-2 text-sm text-foreground outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring/50';

function formatETA(value) {
  if (value == null) return '--';
  if (value <= 0) return '0s';
  const h = Math.floor(value / 3600);
  const m = Math.floor((value % 3600) / 60);
  const s = value % 60;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}

function getStatusMeta(status, progress) {
  const raw = String(status || '').toLowerCase();
  if (raw.includes('error')) return { label: 'Error', cls: 'status-error', icon: Clock };
  if (progress >= 1 || raw.includes('seed')) return { label: 'Seeding', cls: 'status-seeding', icon: CheckCircle2 };
  if (raw.includes('pause')) return { label: 'Paused', cls: 'status-paused', icon: Pause };
  if (raw.includes('metadata')) return { label: 'Metadata', cls: 'status-metadata', icon: Clock };
  return { label: 'Downloading', cls: 'status-downloading', icon: Download };
}

function MiniGraph({ tone }) {
  const pathMap = {
    blue: 'M2,34 C15,28 21,18 33,22 C45,26 53,10 67,16 C81,22 88,11 102,14 C114,17 123,7 132,10',
    purple: 'M2,34 C16,32 22,22 35,26 C48,30 57,13 70,18 C83,23 90,18 102,22 C115,25 122,15 132,17',
    green: 'M2,32 C16,27 24,29 37,20 C50,13 58,18 71,14 C85,10 93,14 108,9 C118,7 124,7 132,8',
    yellow: 'M2,31 C15,24 24,28 36,20 C49,12 58,19 72,14 C84,10 95,8 108,13 C120,16 125,10 132,10',
  };

  return (
    <svg viewBox="0 0 134 40" className="h-10 w-full">
      <path d={pathMap[tone]} className={`mini-graph-${tone}`} fill="none" strokeWidth="2.2" strokeLinecap="round" />
    </svg>
  );
}

function StatCard({ title, value, subtitle, icon: Icon, tone }) {
  return (
    <Card className={cn('premium-card border-border/60', `premium-card-${tone}`)}>
      <CardContent className="p-4">
        <div className="mb-3 flex items-start justify-between">
          <p className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{title}</p>
          <div className="rounded-xl border border-border/70 bg-background/45 p-2">
            <Icon className="h-4 w-4" />
          </div>
        </div>
        <p className="text-3xl font-bold tracking-tight">{value}</p>
        <p className="mb-1 mt-1 text-xs text-muted-foreground">{subtitle}</p>
        <MiniGraph tone={tone} />
      </CardContent>
    </Card>
  );
}

function QueueItem({ label, count, active }) {
  return (
    <div className={cn('flex items-center justify-between rounded-lg px-2 py-1.5 text-sm transition-colors', active ? 'bg-primary/12 text-foreground' : 'text-muted-foreground')}>
      <span>{label}</span>
      <span className="font-semibold text-primary">{count}</span>
    </div>
  );
}

function StatsLine({ label, value }) {
  return (
    <div className="flex items-center justify-between text-sm">
      <span className="text-muted-foreground">{label}</span>
      <span className="font-medium">{value}</span>
    </div>
  );
}

function SpeedGraph({ dlSpeed, ulSpeed }) {
  const mx = Math.max(1, dlSpeed, ulSpeed);
  const dlRatio = Math.max(0.2, dlSpeed / mx);
  const ulRatio = Math.max(0.1, ulSpeed / mx);

  const buildPath = (ratio, phase) => {
    const points = [];
    for (let i = 0; i <= 12; i += 1) {
      const x = i * 36;
      const wave = Math.sin(i * 0.55 + phase) * 12 + Math.cos(i * 0.24 + phase) * 6;
      const y = 200 - (i * 10 * ratio) - (wave * ratio);
      const clamped = Math.max(24, Math.min(210, y));
      points.push(`${x},${clamped.toFixed(1)}`);
    }
    return `M ${points.join(' L ')}`;
  };

  const dlPath = buildPath(dlRatio, 0.2);
  const ulPath = buildPath(ulRatio, 1.1);

  return (
    <div className="rounded-2xl border border-border/70 bg-card/55 p-4">
      <p className="mb-3 text-xs font-semibold uppercase tracking-wider text-muted-foreground">Speed Graph</p>
      <svg viewBox="0 0 432 232" className="h-52 w-full">
        <defs>
          <linearGradient id="dlArea" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="hsl(204 92% 57%)" stopOpacity="0.35" />
            <stop offset="100%" stopColor="hsl(204 92% 57%)" stopOpacity="0.02" />
          </linearGradient>
          <linearGradient id="ulArea" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="hsl(271 81% 62%)" stopOpacity="0.3" />
            <stop offset="100%" stopColor="hsl(271 81% 62%)" stopOpacity="0.02" />
          </linearGradient>
        </defs>
        {[0, 1, 2, 3, 4].map((i) => (
          <line key={`h-${i}`} x1="0" y1={32 + i * 44} x2="432" y2={32 + i * 44} stroke="hsl(var(--border))" strokeWidth="1" opacity="0.45" />
        ))}
        {[0, 1, 2, 3, 4, 5, 6].map((i) => (
          <line key={`v-${i}`} x1={i * 72} y1="20" x2={i * 72} y2="210" stroke="hsl(var(--border))" strokeWidth="1" opacity="0.35" />
        ))}

        <path d={`${dlPath} L 432,210 L 0,210 Z`} fill="url(#dlArea)" />
        <path d={`${ulPath} L 432,210 L 0,210 Z`} fill="url(#ulArea)" />
        <path d={dlPath} stroke="hsl(204 92% 57%)" fill="none" strokeWidth="2.8" strokeLinecap="round" />
        <path d={ulPath} stroke="hsl(271 81% 62%)" fill="none" strokeWidth="2.4" strokeLinecap="round" />
      </svg>
      <div className="mt-3 flex gap-5 text-xs text-muted-foreground">
        <span className="flex items-center gap-2"><span className="h-2 w-2 rounded-full bg-[hsl(204_92%_57%)]" />Download</span>
        <span className="flex items-center gap-2"><span className="h-2 w-2 rounded-full bg-[hsl(271_81%_62%)]" />Upload</span>
      </div>
    </div>
  );
}

function TorrentExpanded({ torrent, tab, onTab }) {
  const files = torrent.files || [];
  const trackers = torrent.trackers || [];
  const ratio = torrent.downloaded > 0 ? (torrent.uploaded / torrent.downloaded).toFixed(2) : '0.00';
  const availability = Math.max(0, Math.min(100, Math.round((torrent.num_seeds / Math.max(1, torrent.num_peers || torrent.num_seeds)) * 100)));

  return (
    <div className="rounded-2xl border border-border/65 bg-card/45 p-4">
      <Tabs value={tab} onValueChange={onTab}>
        <TabsList className="grid w-full grid-cols-7 bg-background/50">
          <TabsTrigger value="overview">Overview</TabsTrigger>
          <TabsTrigger value="files">Files</TabsTrigger>
          <TabsTrigger value="trackers">Trackers</TabsTrigger>
          <TabsTrigger value="peers">Peers</TabsTrigger>
          <TabsTrigger value="speed">Speed</TabsTrigger>
          <TabsTrigger value="activity">Activity</TabsTrigger>
          <TabsTrigger value="chunks">Chunks</TabsTrigger>
        </TabsList>

        <TabsContent value="overview" className="mt-4 grid gap-4 xl:grid-cols-[1.05fr_1.35fr_1fr]">
          <div className="rounded-2xl border border-border/70 bg-card/60 p-4">
            <p className="mb-3 text-xs font-semibold uppercase tracking-wider text-muted-foreground">Transfer Stats</p>
            <div className="space-y-2.5">
              <StatsLine label="Downloaded" value={`${formatBytes(torrent.downloaded)} / ${formatBytes(torrent.total_size)}`} />
              <StatsLine label="Uploaded" value={formatBytes(torrent.uploaded)} />
              <StatsLine label="Ratio" value={ratio} />
              <StatsLine label="Availability" value={`${availability}%`} />
            </div>
            <div className="mt-4 h-2 overflow-hidden rounded-full bg-muted">
              <div className="h-full rounded-full bg-gradient-to-r from-emerald-400 to-green-500" style={{ width: `${availability}%` }} />
            </div>
          </div>

          <SpeedGraph dlSpeed={torrent.dl_speed} ulSpeed={torrent.ul_speed} />

          <div className="rounded-2xl border border-border/70 bg-card/60 p-4">
            <p className="mb-3 text-xs font-semibold uppercase tracking-wider text-muted-foreground">File List</p>
            <div className="space-y-2">
              {(files.length ? files : [{ path: 'No file metadata yet', size: 0 }]).slice(0, 6).map((f, idx) => (
                <div key={`${f.path}-${idx}`} className="flex items-center justify-between rounded-lg border border-border/50 bg-background/50 px-3 py-2 text-sm">
                  <span className="truncate text-muted-foreground">{f.path}</span>
                  <span className="ml-3 whitespace-nowrap">{formatBytes(f.size || 0)}</span>
                </div>
              ))}
            </div>
          </div>
        </TabsContent>

        <TabsContent value="files" className="mt-4 rounded-2xl border border-border/70 bg-card/60 p-4">
          <div className="space-y-2">
            {(files.length ? files : [{ path: 'No file metadata yet', size: 0 }]).map((f, idx) => (
              <div key={`${f.path}-${idx}`} className="flex items-center justify-between rounded-lg border border-border/50 bg-background/50 px-3 py-2 text-sm">
                <span className="truncate text-muted-foreground">{f.path}</span>
                <span>{formatBytes(f.size || 0)}</span>
              </div>
            ))}
          </div>
        </TabsContent>

        <TabsContent value="trackers" className="mt-4 rounded-2xl border border-border/70 bg-card/60 p-4">
          <div className="space-y-2">
            {(trackers.length ? trackers : [{ url: 'No trackers available', status: 'unknown' }]).map((t, idx) => (
              <div key={`${t.url}-${idx}`} className="rounded-lg border border-border/50 bg-background/50 px-3 py-2 text-sm">
                <p className="truncate">{t.url}</p>
                <p className="text-xs text-muted-foreground">Status: {t.status || 'unknown'}</p>
              </div>
            ))}
          </div>
        </TabsContent>

        <TabsContent value="peers" className="mt-4 rounded-2xl border border-border/70 bg-card/60 p-4">
          <div className="grid gap-3 sm:grid-cols-3">
            <PeerCard label="Connected Peers" value={torrent.num_peers} color="text-blue-400" />
            <PeerCard label="Seeders" value={torrent.num_seeds} color="text-green-400" />
            <PeerCard label="Leechers" value={torrent.num_leechers} color="text-red-400" />
          </div>
        </TabsContent>

        <TabsContent value="speed" className="mt-4">
          <SpeedGraph dlSpeed={torrent.dl_speed} ulSpeed={torrent.ul_speed} />
        </TabsContent>

        <TabsContent value="activity" className="mt-4 rounded-2xl border border-border/70 bg-card/60 p-4 font-mono text-xs">
          <div className="max-h-[300px] space-y-1 overflow-y-auto pr-2 scrollbar-premium">
            {(torrent.logs && torrent.logs.length > 0) ? (
              torrent.logs.map((log, idx) => (
                <div key={idx} className="flex gap-2 leading-relaxed animate-in fade-in slide-in-from-left-2 duration-300">
                  <span className="text-muted-foreground whitespace-nowrap">[{new Date(log.timestamp).toLocaleTimeString()}]</span>
                  <span className={cn(
                    'font-bold w-12',
                    log.level === 'error' ? 'text-red-400' : 
                    log.level === 'warn' ? 'text-yellow-400' : 
                    'text-emerald-400'
                  )}>{log.level.toUpperCase()}</span>
                  <span className="text-foreground/90">{log.message}</span>
                </div>
              ))
            ) : (
              <div className="py-10 text-center text-muted-foreground italic">No activity recorded yet...</div>
            )}
          </div>
        </TabsContent>

        <TabsContent value="chunks" className="mt-4 rounded-2xl border border-border/70 bg-card/60 p-4">
          <div className="grid grid-cols-12 gap-1.5">
            {Array.from({ length: 72 }).map((_, idx) => {
              const done = idx < Math.floor((torrent.progress || 0) * 72);
              return <div key={idx} className={cn('h-2 rounded-full', done ? 'bg-gradient-to-r from-emerald-400 to-cyan-400' : 'bg-muted')} />;
            })}
          </div>
        </TabsContent>
      </Tabs>
    </div>
  );
}

function PeerCard({ label, value, color }) {
  return (
    <div className="rounded-xl border border-border/60 bg-background/45 p-3">
      <p className="text-xs uppercase tracking-wider text-muted-foreground">{label}</p>
      <p className={cn('mt-1 text-2xl font-semibold', color)}>{value}</p>
    </div>
  );
}

function SettingsView({ settings, onSave }) {
  const [formData, setFormData] = useState(settings || {
    server: { port: 6977, bind: '127.0.0.1' },
    downloads: { directory: '', max_peers: 200 },
    limits: { download_kbps: 0, upload_kbps: 0 },
  });

  useEffect(() => {
    if (settings) setFormData(settings);
  }, [settings]);

  if (!settings) return <div className="py-20 text-center text-muted-foreground">Loading settings...</div>;

  const pickDir = async () => {
    try {
      const path = await selectDirectory();
      setFormData((prev) => ({ ...prev, downloads: { ...prev.downloads, directory: path } }));
      toast.success('Directory selected');
    } catch (err) {
      const message = String(err?.message || '');
      if (message.toLowerCase().includes('cancel')) {
        return;
      }
      toast.error('Native directory picker is unavailable in your browser. Please type the path manually.');
    }
  };

  return (
    <Card className="border border-border/60 bg-card/65 backdrop-blur">
      <CardHeader>
        <CardTitle>Settings</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="grid gap-4 md:grid-cols-2">
          <div className="space-y-2 md:col-span-2">
            <label className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">Download Directory</label>
            <div className="flex gap-2">
              <Input value={formData.downloads.directory} onChange={(e) => setFormData({ ...formData, downloads: { ...formData.downloads, directory: e.target.value } })} />
              <Button type="button" variant="outline" size="icon" onClick={pickDir}>
                <FolderOpen className="h-4 w-4" />
              </Button>
            </div>
          </div>

          <Field label="Server Port" value={formData.server.port} onChange={(v) => setFormData({ ...formData, server: { ...formData.server, port: Number(v) || 0 } })} />
          <Field label="Max Peers" value={formData.downloads.max_peers} onChange={(v) => setFormData({ ...formData, downloads: { ...formData.downloads, max_peers: Number(v) || 0 } })} />
          <Field label="Download Limit (KB/s)" value={formData.limits.download_kbps} onChange={(v) => setFormData({ ...formData, limits: { ...formData.limits, download_kbps: Number(v) || 0 } })} />
          <Field label="Upload Limit (KB/s)" value={formData.limits.upload_kbps} onChange={(v) => setFormData({ ...formData, limits: { ...formData.limits, upload_kbps: Number(v) || 0 } })} />
        </div>

        <div className="flex gap-2">
          <Button onClick={() => { onSave(formData); toast.success('Settings saved'); }}>Save</Button>
          <Button variant="outline" onClick={() => setFormData(settings)}>Reset</Button>
        </div>
      </CardContent>
    </Card>
  );
}

function Field({ label, value, onChange }) {
  return (
    <div className="space-y-2">
      <label className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{label}</label>
      <Input type="number" value={value} onChange={(e) => onChange(e.target.value)} />
    </div>
  );
}

export default function App() {
  const { torrents, globalStats, connected } = useFeTorrentStream();
  const [theme, setTheme] = useState(initialTheme);
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [view, setView] = useState('transfer');
  const [filter, setFilter] = useState('All Torrents');
  const [search, setSearch] = useState('');
  const [isAddOpen, setIsAddOpen] = useState(false);
  const [settings, setSettings] = useState(null);
  const [addingMagnet, setAddingMagnet] = useState('');
  const [addingFile, setAddingFile] = useState(null);
  const [addingDir, setAddingDir] = useState('');
  const [expandedId, setExpandedId] = useState(null);
  const [expandedTab, setExpandedTab] = useState('overview');

  useEffect(() => {
    localStorage.setItem('fetorrent-theme', theme);
    document.documentElement.classList.toggle('dark', theme === 'dark');
    document.documentElement.classList.toggle('light', theme === 'light');
    document.body.classList.toggle('dark', theme === 'dark');
    document.body.classList.toggle('light', theme === 'light');
    document.documentElement.setAttribute('data-theme', theme);
  }, [theme]);

  useEffect(() => {
    fetchSettings()
      .then((cfg) => {
        setSettings(cfg);
        if (!addingDir && cfg?.downloads?.directory) {
          setAddingDir(cfg.downloads.directory);
        }
      })
      .catch(console.error);
  }, []);

  const counts = useMemo(() => {
    const all = torrents.length;
    const downloading = torrents.filter((t) => String(t.status).toLowerCase().includes('download')).length;
    const active = torrents.filter((t) => t.dl_speed > 0 || t.ul_speed > 0).length;
    const completed = torrents.filter((t) => t.progress >= 1).length;
    return { all, downloading, active, completed };
  }, [torrents]);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    return torrents.filter((t) => {
      if (filter === 'Downloading' && !String(t.status).toLowerCase().includes('download')) return false;
      if (filter === 'Completed' && t.progress < 1) return false;
      if (q && !String(t.name || '').toLowerCase().includes(q)) return false;
      return true;
    });
  }, [torrents, filter, search]);

  const chooseDir = async (setter, current) => {
    try {
      const path = await selectDirectory();
      setter(path);
      toast.success('Directory selected');
    } catch (err) {
      const message = String(err?.message || '');
      if (message.toLowerCase().includes('cancel')) {
        return;
      }
      toast.error('Native directory picker is unavailable in your browser. Please type the path manually.');
    }
  };

  const onAddTorrent = async (event) => {
    event.preventDefault();

    const formData = new FormData();
    if (addingMagnet.trim()) {
      formData.append('magnet', addingMagnet.trim());
    } else if (addingFile) {
      formData.append('file', addingFile);
    } else {
      toast.error('Please provide a magnet URI or a .torrent file');
      return;
    }

    if (addingDir.trim()) formData.append('dir', addingDir.trim());

    toast.promise(addTorrent(formData), {
      loading: 'Initializing swarm...',
      success: 'Torrent added',
      error: 'Failed to add torrent',
    });

    setAddingMagnet('');
    setAddingFile(null);
    setIsAddOpen(false);
  };

  const queueBytes = formatBytes(torrents.reduce((acc, t) => acc + (t.total_size || 0), 0));

  return (
    <div className={cn('min-h-screen bg-background text-foreground', theme)}>
      <Toaster position="top-right" theme={theme} />

      <div className="premium-shell flex h-screen overflow-hidden">
        {sidebarOpen && <div className="fixed inset-0 z-20 bg-black/40 lg:hidden" onClick={() => setSidebarOpen(false)} />}

        <aside className={cn('premium-sidebar fixed z-30 h-full w-72 border-r border-border/60 px-4 py-5 transition-transform lg:static', sidebarOpen ? 'translate-x-0' : '-translate-x-full lg:translate-x-0')}>
          <div className="flex h-full flex-col gap-5">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-3">
                <div className="logo-dot flex h-10 w-10 items-center justify-center rounded-xl">
                  <Activity className="h-5 w-5" />
                </div>
                <p className="text-xl font-semibold tracking-tight text-foreground">FeTorrent</p>
              </div>
              <Button variant="ghost" size="icon" className="lg:hidden" onClick={() => setSidebarOpen(false)}>
                <X className="h-4 w-4" />
              </Button>
            </div>

            <nav className="space-y-2">
              <button onClick={() => setView('transfer')} className={cn('nav-item', view === 'transfer' && 'nav-item-active')}>Transfer</button>
              <button onClick={() => setView('settings')} className={cn('nav-item', view === 'settings' && 'nav-item-active')}>Settings</button>
            </nav>

            <div className="glass-panel rounded-2xl border border-border/60 p-3">
              <p className="mb-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">Queue</p>
              <div className="space-y-1">
                <QueueItem label="All" count={counts.all} active={filter === 'All Torrents'} />
                <QueueItem label="Downloading" count={counts.downloading} active={filter === 'Downloading'} />
                <QueueItem label="Active" count={counts.active} active={false} />
                <QueueItem label="Completed" count={counts.completed} active={filter === 'Completed'} />
              </div>
            </div>

            <div className="mt-auto rounded-2xl border border-emerald-500/30 bg-emerald-500/10 p-3 text-sm">
              <div className="mb-1 flex items-center gap-2 font-semibold text-emerald-400">
                <span className={cn('h-2.5 w-2.5 rounded-full', connected ? 'bg-emerald-400' : 'bg-red-500')} />
                {connected ? 'Connected' : 'Disconnected'}
              </div>
              <p className="text-xs text-emerald-300/80">All systems operational</p>
            </div>
          </div>
        </aside>

        <main className="flex flex-1 flex-col overflow-hidden">
          <header className="border-b border-border/60 bg-card/60 px-6 py-4 backdrop-blur-xl">
            <div className="mb-4 flex items-center justify-between gap-4">
              <div className="flex items-center gap-3">
                <Button variant="ghost" size="icon" className="lg:hidden" onClick={() => setSidebarOpen((v) => !v)}>
                  <Menu className="h-5 w-5" />
                </Button>
                <h1 className="text-2xl font-semibold tracking-tight text-foreground">Torrent Control</h1>
              </div>

              <div className="flex items-center gap-2">
                <Button variant="outline" size="icon" onClick={() => setTheme((v) => (v === 'dark' ? 'light' : 'dark'))} title="Toggle light/dark mode">
                  {theme === 'dark' ? <Sun className="h-4 w-4" /> : <Moon className="h-4 w-4" />}
                </Button>

                <Dialog open={isAddOpen} onOpenChange={setIsAddOpen}>
                  <DialogTrigger asChild>
                    <Button className="gap-2 bg-primary text-primary-foreground hover:brightness-105">
                      <Plus className="h-4 w-4" />
                      Add Torrent
                    </Button>
                  </DialogTrigger>
                  <DialogContent className="w-[min(94vw,960px)] sm:max-w-3xl">
                    <DialogHeader>
                      <DialogTitle>Add Torrent</DialogTitle>
                      <DialogDescription>Add magnet link or .torrent file and choose location.</DialogDescription>
                    </DialogHeader>
                    <form onSubmit={onAddTorrent} className="space-y-4">
                      <Tabs defaultValue="magnet">
                        <TabsList className="grid w-full grid-cols-2">
                          <TabsTrigger value="magnet">Magnet URI</TabsTrigger>
                          <TabsTrigger value="file">.torrent File</TabsTrigger>
                        </TabsList>
                        <TabsContent value="magnet" className="mt-4">
                          <textarea
                            placeholder="magnet:?xt=urn:btih:..."
                            className={magnetInputClass}
                            value={addingMagnet}
                            onChange={(e) => {
                              setAddingMagnet(e.target.value);
                              setAddingFile(null);
                            }}
                          />
                        </TabsContent>
                        <TabsContent value="file" className="mt-4">
                          <Input
                            type="file"
                            accept=".torrent"
                            onChange={(e) => {
                              if (e.target.files?.[0]) {
                                setAddingFile(e.target.files[0]);
                                setAddingMagnet('');
                              }
                            }}
                          />
                        </TabsContent>
                      </Tabs>

                      <div className="space-y-2">
                        <label className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">Download Directory</label>
                        <div className="flex gap-2">
                          <Input
                            value={addingDir}
                            onChange={(e) => setAddingDir(e.target.value)}
                            placeholder={settings?.downloads?.directory || 'Use default download directory'}
                          />
                          <Button type="button" variant="outline" size="icon" onClick={async () => chooseDir(setAddingDir, addingDir || settings?.downloads?.directory || '')}>
                            <FolderOpen className="h-4 w-4" />
                          </Button>
                        </div>
                      </div>

                      <DialogFooter>
                        <Button type="submit" className="w-full">+ Add Torrent</Button>
                      </DialogFooter>
                    </form>
                  </DialogContent>
                </Dialog>
              </div>
            </div>

            <div className="flex flex-wrap items-center gap-3">
              <div className="relative min-w-[280px] flex-1">
                <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
                <Input value={search} onChange={(e) => setSearch(e.target.value)} placeholder="Search torrents..." className="pl-9" />
              </div>

              <div className="segmented flex rounded-xl border border-border/60 bg-background/50 p-1">
                {['All Torrents', 'Downloading', 'Completed'].map((label) => (
                  <button key={label} className={cn('segmented-item', filter === label && 'segmented-item-active')} onClick={() => setFilter(label)}>
                    {label}
                  </button>
                ))}
              </div>
            </div>
          </header>

          <div className="flex-1 overflow-y-auto px-6 py-5">
            {view === 'settings' ? (
              <SettingsView settings={settings} onSave={updateSettings} />
            ) : (
              <div className="space-y-5">
                <section className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
                  <StatCard title="Download Speed" value={`${formatBytes(globalStats.dl_speed)}/s`} subtitle={`${counts.downloading} downloading`} icon={Download} tone="blue" />
                  <StatCard title="Upload Speed" value={`${formatBytes(globalStats.ul_speed)}/s`} subtitle={`${counts.active} active`} icon={Upload} tone="purple" />
                  <StatCard title="Active Torrents" value={String(globalStats.active_torrents)} subtitle="Live sessions" icon={Users} tone="green" />
                  <StatCard title="Queued Size" value={queueBytes} subtitle={`${counts.all} in queue`} icon={HardDrive} tone="yellow" />
                </section>

                <section className="overflow-hidden rounded-2xl border border-border/60 bg-card/55 backdrop-blur-xl">
                  <div className="grid grid-cols-[2.2fr_1.4fr_1fr_1fr_1.2fr_1fr_1fr_130px] gap-4 border-b border-border/50 px-5 py-4 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                    <span>Name</span>
                    <span>Progress</span>
                    <span>Download</span>
                    <span>Upload</span>
                    <span>Peers</span>
                    <span>Status</span>
                    <span>ETA</span>
                    <span className="text-right">Actions</span>
                  </div>

                  <div className="divide-y divide-border/35">
                    {filtered.length === 0 ? (
                      <div className="px-6 py-14 text-center text-sm text-muted-foreground">No torrents found.</div>
                    ) : (
                      filtered.map((torrent) => {
                        const status = getStatusMeta(torrent.status, torrent.progress);
                        const StatusIcon = status.icon;
                        const expanded = expandedId === torrent.id;
                        const paused = String(torrent.status || '').toLowerCase().includes('pause');

                        return (
                          <div key={torrent.id} className="torrent-row">
                            <div
                              className="grid cursor-pointer grid-cols-[2.2fr_1.4fr_1fr_1fr_1.2fr_1fr_1fr_130px] gap-4 px-5 py-4 transition-colors hover:bg-muted/25"
                              onClick={() => {
                                setExpandedId(expanded ? null : torrent.id);
                                setExpandedTab('overview');
                              }}
                            >
                              <div className="min-w-0">
                                <div className="flex items-center gap-3">
                                  <div className="rounded-lg border border-border/60 bg-background/55 p-2">
                                    <FileText className="h-4 w-4 text-primary" />
                                  </div>
                                  <div className="min-w-0">
                                    <p className="truncate font-semibold">{torrent.name}</p>
                                    <p className="text-xs text-muted-foreground">{formatBytes(torrent.downloaded)} / {formatBytes(torrent.total_size)}</p>
                                  </div>
                                </div>
                              </div>

                              <div className="pr-2">
                                <p className="mb-1 text-xs font-semibold text-primary">{(torrent.progress * 100).toFixed(1)}%</p>
                                <div className="h-3 overflow-hidden rounded-full bg-muted">
                                  <div className="progress-premium h-full rounded-full" style={{ width: `${Math.max(0, Math.min(100, torrent.progress * 100))}%` }} />
                                </div>
                              </div>

                              <div className="text-sm font-semibold text-[hsl(204_92%_57%)]">{formatBytes(torrent.dl_speed)}/s</div>
                              <div className="text-sm font-semibold text-[hsl(271_81%_62%)]">{formatBytes(torrent.ul_speed)}/s</div>

                              <div className="text-sm">
                                <span className="font-medium">Seeders: {torrent.num_seeds}</span>
                                <span className="text-muted-foreground"> | Leechers: {torrent.num_leechers}</span>
                              </div>

                              <div>
                                <Badge className={cn('border font-medium', status.cls)}>
                                  <StatusIcon className="mr-1 h-3.5 w-3.5" />
                                  {status.label}
                                </Badge>
                              </div>

                              <div className="text-sm font-medium">{status.label === 'Seeding' ? '--' : formatETA(torrent.eta_secs)}</div>

                              <div className="flex items-center justify-end gap-1" onClick={(e) => e.stopPropagation()}>
                                <Button variant="ghost" size="icon" onClick={() => (paused ? resumeTorrent(torrent.id) : pauseTorrent(torrent.id))}>
                                  {paused ? <Play className="h-4 w-4" /> : <Pause className="h-4 w-4" />}
                                </Button>
                                <Button variant="ghost" size="icon" onClick={() => removeTorrent(torrent.id)}>
                                  <Trash2 className="h-4 w-4" />
                                </Button>
                                <Button variant="ghost" size="icon">
                                  <MoreHorizontal className="h-4 w-4" />
                                </Button>
                              </div>
                            </div>

                            {expanded && (
                              <div className="px-5 pb-5">
                                <TorrentExpanded torrent={torrent} tab={expandedTab} onTab={setExpandedTab} />
                              </div>
                            )}
                          </div>
                        );
                      })
                    )}
                  </div>
                </section>
              </div>
            )}
          </div>
        </main>
      </div>
    </div>
  );
}
