// GNOME 45 and later. GNOME tells no app which window has the focus; this tells the app over
// D-Bus when it asks.
import Gio from 'gi://Gio'
import Shell from 'gi://Shell'
import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js'

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

export default class TaurilanziExtension extends Extension {
  enable() {
    this._object = Gio.DBusExportedObject.wrapJSObject(INTERFACE, this)
    this._object.export(Gio.DBus.session, PATH)
    this._owner = Gio.bus_own_name_on_connection(Gio.DBus.session, NAME, Gio.BusNameOwnerFlags.NONE, null, null)
  }

  disable() {
    this._object?.unexport()
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
    return [app?.get_id() ?? '', window.get_pid()]
  }
}
