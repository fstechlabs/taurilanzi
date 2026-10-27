// GNOME 42 to 44, which load extensions as scripts instead of modules.
const { Gio, Shell } = imports.gi

const NAME = 'com.taurilanzi.Shell'
const PATH = '/com/taurilanzi/Shell'
const INTERFACE = `<node>
  <interface name="${NAME}">
    <method name="FocusedApp">
      <arg type="s" direction="out" name="id"/>
      <arg type="u" direction="out" name="pid"/>
    </method>
  </interface>
</node>`

class TaurilanziExtension {
  enable() {
    this._object = Gio.DBusExportedObject.wrapJSObject(INTERFACE, this)
    this._object.export(Gio.DBus.session, PATH)
    this._owner = Gio.bus_own_name_on_connection(Gio.DBus.session, NAME, Gio.BusNameOwnerFlags.NONE, null, null)
  }

  disable() {
    if (this._object) {
      this._object.unexport()
    }
    this._object = null
    if (this._owner) {
      Gio.bus_unown_name(this._owner)
    }
    this._owner = 0
  }

  FocusedApp() {
    const window = global.display.focus_window
    if (!window) {
      return ['', 0]
    }
    const app = Shell.WindowTracker.get_default().get_window_app(window)
    return [app ? app.get_id() : '', window.get_pid()]
  }
}

function init() {
  return new TaurilanziExtension()
}
