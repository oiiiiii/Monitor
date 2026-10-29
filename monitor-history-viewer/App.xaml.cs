using System.Windows;

namespace MonitorHistoryViewer;

public partial class App : Application
{
    protected override void OnStartup(StartupEventArgs e)
    {
        if (!SingleInstanceHelper.EnsureSingleInstance())
        {
            Shutdown();
            return;
        }

        base.OnStartup(e);
    }
}