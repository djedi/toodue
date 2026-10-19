import UIKit
import Capacitor

/// Registers the app's local plugins with the Capacitor bridge.
class MainViewController: CAPBridgeViewController {
    override open func capacitorDidLoad() {
        bridge?.registerPluginInstance(NativeTabsPlugin())
    }
}

/// A floating native tab bar laid over the web view, drawn with the system
/// Liquid Glass material on iOS 26+ (a blur on older iOS). The web app owns
/// navigation: it sends the tabs, selection and badges, and gets
/// `tabSelected` events back.
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

    private var bar: GlassTabBar?
    private var ids: [String] = []
    private var selectedId: String?
    private var badges: JSObject = [:]

    @objc func show(_ call: CAPPluginCall) {
        let tabs = call.getArray("tabs", JSObject.self) ?? []
        let selected = call.getString("selected")
        DispatchQueue.main.async {
            let bar = self.bar ?? self.makeBar()
            self.ids = tabs.compactMap { $0["id"] as? String }
            bar.setItems(tabs.map { ($0["label"] as? String ?? "", $0["icon"] as? String ?? "circle") })
            self.selectedId = selected ?? self.selectedId
            if let tint = call.getString("tint").flatMap(UIColor.init(hex:)) {
                bar.tintColor = tint
            }
            self.refresh()
            bar.isHidden = false
            call.resolve()
        }
    }

    @objc func hide(_ call: CAPPluginCall) {
        DispatchQueue.main.async {
            self.bar?.isHidden = true
            call.resolve()
        }
    }

    @objc func select(_ call: CAPPluginCall) {
        let id = call.getString("id")
        DispatchQueue.main.async {
            self.selectedId = id
            self.refresh()
            call.resolve()
        }
    }

    @objc func setBadges(_ call: CAPPluginCall) {
        let badges = call.getObject("badges") ?? [:]
        DispatchQueue.main.async {
            self.badges = badges
            self.refresh()
            call.resolve()
        }
    }

    private func refresh() {
        guard let bar = bar else { return }
        bar.selectedIndex = selectedId.flatMap { ids.firstIndex(of: $0) }
        bar.badges = ids.map { (badges[$0] as? NSNumber)?.intValue ?? 0 }
    }

    private func makeBar() -> GlassTabBar {
        let bar = GlassTabBar()
        bar.onSelect = { [weak self] index in
            guard let self = self, index < self.ids.count else { return }
            self.selectedId = self.ids[index]
            self.refresh()
            self.notifyListeners("tabSelected", data: ["id": self.ids[index]])
        }
        if let host = bridge?.viewController?.view {
            host.addSubview(bar)
            bar.translatesAutoresizingMaskIntoConstraints = false
            NSLayoutConstraint.activate([
                bar.leadingAnchor.constraint(equalTo: host.leadingAnchor, constant: 16),
                bar.trailingAnchor.constraint(equalTo: host.trailingAnchor, constant: -16),
                bar.bottomAnchor.constraint(equalTo: host.safeAreaLayoutGuide.bottomAnchor, constant: -4),
                bar.heightAnchor.constraint(equalToConstant: 62)
            ])
        }
        self.bar = bar
        return bar
    }
}

/// Capsule of tab buttons on a glass background, styled after the iOS 26 tab bar.
final class GlassTabBar: UIView {
    var onSelect: ((Int) -> Void)?
    var selectedIndex: Int? { didSet { updateAppearance() } }
    var badges: [Int] = [] { didSet { updateAppearance() } }

    private let background: UIVisualEffectView
    private let indicator = UIView()
    private let stack = UIStackView()
    private var buttons: [UIButton] = []
    private var badgeLabels: [UILabel] = []

    override init(frame: CGRect) {
        if #available(iOS 26.0, *) {
            let glass = UIGlassEffect()
            glass.isInteractive = true
            background = UIVisualEffectView(effect: glass)
        } else {
            background = UIVisualEffectView(effect: UIBlurEffect(style: .systemChromeMaterial))
        }
        super.init(frame: frame)
        background.clipsToBounds = true
        background.translatesAutoresizingMaskIntoConstraints = false
        addSubview(background)
        indicator.backgroundColor = UIColor.label.withAlphaComponent(0.08)
        background.contentView.addSubview(indicator)
        stack.axis = .horizontal
        stack.distribution = .fillEqually
        stack.translatesAutoresizingMaskIntoConstraints = false
        background.contentView.addSubview(stack)
        NSLayoutConstraint.activate([
            background.leadingAnchor.constraint(equalTo: leadingAnchor),
            background.trailingAnchor.constraint(equalTo: trailingAnchor),
            background.topAnchor.constraint(equalTo: topAnchor),
            background.bottomAnchor.constraint(equalTo: bottomAnchor),
            stack.leadingAnchor.constraint(equalTo: background.contentView.leadingAnchor, constant: 4),
            stack.trailingAnchor.constraint(equalTo: background.contentView.trailingAnchor, constant: -4),
            stack.topAnchor.constraint(equalTo: background.contentView.topAnchor, constant: 4),
            stack.bottomAnchor.constraint(equalTo: background.contentView.bottomAnchor, constant: -4)
        ])
        layer.shadowColor = UIColor.black.cgColor
        layer.shadowOpacity = 0.12
        layer.shadowRadius = 16
        layer.shadowOffset = CGSize(width: 0, height: 4)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    func setItems(_ items: [(label: String, symbol: String)]) {
        buttons.forEach { $0.removeFromSuperview() }
        buttons = []
        badgeLabels = []
        for (index, item) in items.enumerated() {
            var config = UIButton.Configuration.plain()
            config.image = UIImage(systemName: item.symbol,
                                   withConfiguration: UIImage.SymbolConfiguration(pointSize: 18, weight: .medium))
            config.imagePlacement = .top
            config.imagePadding = 3
            var title = AttributedString(item.label)
            title.font = .systemFont(ofSize: 10, weight: .semibold)
            config.attributedTitle = title
            let button = UIButton(configuration: config)
            button.accessibilityLabel = item.label
            button.addAction(UIAction { [weak self] _ in self?.onSelect?(index) }, for: .touchUpInside)

            let badge = UILabel()
            badge.font = .systemFont(ofSize: 10, weight: .bold)
            badge.textColor = .white
            badge.textAlignment = .center
            badge.backgroundColor = .systemRed
            badge.layer.cornerRadius = 8
            badge.clipsToBounds = true
            badge.translatesAutoresizingMaskIntoConstraints = false
            button.addSubview(badge)
            NSLayoutConstraint.activate([
                badge.centerXAnchor.constraint(equalTo: button.centerXAnchor, constant: 14),
                badge.topAnchor.constraint(equalTo: button.topAnchor, constant: 4),
                badge.heightAnchor.constraint(equalToConstant: 16),
                badge.widthAnchor.constraint(greaterThanOrEqualToConstant: 16)
            ])
            stack.addArrangedSubview(button)
            buttons.append(button)
            badgeLabels.append(badge)
        }
        updateAppearance()
    }

    override func tintColorDidChange() {
        super.tintColorDidChange()
        updateAppearance()
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        background.layer.cornerRadius = bounds.height / 2
        layer.shadowPath = UIBezierPath(roundedRect: bounds, cornerRadius: bounds.height / 2).cgPath
        positionIndicator()
    }

    private func updateAppearance() {
        for (index, button) in buttons.enumerated() {
            button.configuration?.baseForegroundColor = index == selectedIndex ? tintColor : .label
            let count = index < badges.count ? badges[index] : 0
            badgeLabels[index].text = count > 0 ? " \(count) " : nil
            badgeLabels[index].isHidden = count == 0
            badgeLabels[index].isAccessibilityElement = false
            button.accessibilityValue = count > 0 ? "\(count) tasks" : nil
            button.accessibilityTraits = index == selectedIndex ? [.button, .selected] : .button
        }
        UIView.animate(withDuration: 0.35, delay: 0, usingSpringWithDamping: 0.8,
                       initialSpringVelocity: 0, options: [.beginFromCurrentState]) {
            self.positionIndicator()
        }
    }

    private func positionIndicator() {
        guard let index = selectedIndex, index < buttons.count else {
            indicator.alpha = 0
            return
        }
        stack.layoutIfNeeded()
        let frame = stack.convert(buttons[index].frame, to: background.contentView)
        indicator.frame = frame
        indicator.layer.cornerRadius = frame.height / 2
        indicator.alpha = 1
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
