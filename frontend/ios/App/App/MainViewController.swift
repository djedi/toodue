import UIKit
import Capacitor

/// Registers the app's local plugins with the Capacitor bridge.
class MainViewController: CAPBridgeViewController {
    override open func capacitorDidLoad() {
        bridge?.registerPluginInstance(NativeTabsPlugin())
    }
}

/// Root controller: a real UITabBarController, so iOS draws its own tab bar
/// (Liquid Glass on iOS 26+). There is only one web view, so each tab is an
/// empty container and the bridge view controller moves into whichever tab
/// is selected. The web app owns navigation; the tab bar just reports taps.
final class TabRootController: UITabBarController, UITabBarControllerDelegate {
    let bridgeController = MainViewController()
    var onSelect: ((Int) -> Void)?

    override func viewDidLoad() {
        super.viewDidLoad()
        delegate = self
        setTabs([("", "circle")])
        setBarHidden(true)
    }

    func setTabs(_ items: [(label: String, symbol: String)]) {
        let current = selectedIndex
        viewControllers = items.map { item in
            let container = UIViewController()
            container.tabBarItem = UITabBarItem(title: item.label, image: UIImage(systemName: item.symbol), selectedImage: nil)
            return container
        }
        select(index: min(current, items.count - 1))
    }

    func select(index: Int) {
        guard let containers = viewControllers, containers.indices.contains(index) else { return }
        selectedIndex = index
        embedBridge(in: containers[index])
    }

    func setBarHidden(_ hidden: Bool) {
        if #available(iOS 18.0, *) {
            setTabBarHidden(hidden, animated: true)
        } else {
            tabBar.isHidden = hidden
        }
    }

    func tabBarController(_ tabBarController: UITabBarController, didSelect viewController: UIViewController) {
        embedBridge(in: viewController)
        onSelect?(selectedIndex)
    }

    private func embedBridge(in container: UIViewController) {
        guard bridgeController.parent !== container else { return }
        if bridgeController.parent != nil {
            bridgeController.willMove(toParent: nil)
            bridgeController.view.removeFromSuperview()
            bridgeController.removeFromParent()
        }
        container.addChild(bridgeController)
        bridgeController.view.frame = container.view.bounds
        bridgeController.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        container.view.addSubview(bridgeController.view)
        bridgeController.didMove(toParent: container)
    }
}

/// Bridges the system tab bar to the web app: it sends the tabs, selection and
/// badges, and gets `tabSelected` events back.
@objc(NativeTabsPlugin)
public class NativeTabsPlugin: CAPPlugin, CAPBridgedPlugin {
    public let identifier = "NativeTabsPlugin"
    public let jsName = "NativeTabs"
    public let pluginMethods: [CAPPluginMethod] = [
        CAPPluginMethod(name: "show", returnType: CAPPluginReturnPromise),
        CAPPluginMethod(name: "hide", returnType: CAPPluginReturnPromise),
        CAPPluginMethod(name: "select", returnType: CAPPluginReturnPromise),
        CAPPluginMethod(name: "setBadges", returnType: CAPPluginReturnPromise)
    ]

    private var ids: [String] = []
    private var badges: JSObject = [:]

    private var root: TabRootController? {
        bridge?.viewController?.tabBarController as? TabRootController
    }

    @objc func show(_ call: CAPPluginCall) {
        let tabs = call.getArray("tabs", JSObject.self) ?? []
        let selected = call.getString("selected")
        let tint = call.getString("tint").flatMap(UIColor.init(hex:))
        DispatchQueue.main.async {
            guard let root = self.root else { return call.reject("tab bar unavailable") }
            let ids = tabs.compactMap { $0["id"] as? String }
            if ids != self.ids {
                self.ids = ids
                root.setTabs(tabs.map { ($0["label"] as? String ?? "", $0["icon"] as? String ?? "circle") })
                root.onSelect = { [weak self] index in
                    guard let self = self, self.ids.indices.contains(index) else { return }
                    self.notifyListeners("tabSelected", data: ["id": self.ids[index]])
                }
            }
            if let tint = tint { root.tabBar.tintColor = tint }
            if let index = selected.flatMap({ ids.firstIndex(of: $0) }) { root.select(index: index) }
            self.applyBadges()
            root.setBarHidden(false)
            call.resolve()
        }
    }

    @objc func hide(_ call: CAPPluginCall) {
        DispatchQueue.main.async {
            self.root?.setBarHidden(true)
            call.resolve()
        }
    }

    @objc func select(_ call: CAPPluginCall) {
        let id = call.getString("id")
        DispatchQueue.main.async {
            if let index = id.flatMap({ self.ids.firstIndex(of: $0) }) { self.root?.select(index: index) }
            call.resolve()
        }
    }

    @objc func setBadges(_ call: CAPPluginCall) {
        let badges = call.getObject("badges") ?? [:]
        DispatchQueue.main.async {
            self.badges = badges
            self.applyBadges()
            call.resolve()
        }
    }

    private func applyBadges() {
        for (index, container) in (root?.viewControllers ?? []).enumerated() where index < ids.count {
            let count = (badges[ids[index]] as? NSNumber)?.intValue ?? 0
            container.tabBarItem.badgeValue = count > 0 ? String(count) : nil
        }
    }
}

private extension UIColor {
    /// Parses `#rrggbb`.
    convenience init?(hex: String) {
        let digits = hex.trimmingCharacters(in: .whitespaces).replacingOccurrences(of: "#", with: "")
        guard digits.count == 6, let value = UInt32(digits, radix: 16) else { return nil }
        self.init(red: CGFloat((value >> 16) & 0xff) / 255,
                  green: CGFloat((value >> 8) & 0xff) / 255,
                  blue: CGFloat(value & 0xff) / 255,
                  alpha: 1)
    }
}
