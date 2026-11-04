# Commercial Preflight

English: [README.md](README.md)

**Windows için yerel öncelikli ticari belge güvence aracı.**

> [!IMPORTANT]
> Commercial Preflight v0.1.0, erken aşamadaki bir açık kaynak yazılımdır.
> Operasyonel kullanıma geçmeden önce kendi sentetik test senaryolarınızla
> doğrulayın. İnsan incelemesini destekler; ticari, muhasebesel veya hukuki
> onayın yerine geçmez.

Commercial Preflight, XLSX ve seçilebilir metin içeren PDF tekliflerini kontrol
eder, yapılandırılabilir şirket politikalarını uygular ve müşteri siparişlerini
kayıtlı teklif revizyonlarıyla karşılaştırır. Ticari farkları belirler, yerel
geçmiş tutar ve belgeleri bulutta işlemeden çalışır.

## Özellik özeti

- Biçimin gerekli kanıtı sunduğu durumlarda teklifleri aritmetik, iskonto,
  ödeme vadesi, geçerlilik, müşteri, metadata ve belge hijyeni sorunlarına karşı
  kontrol eder.
- Teklif revizyonlarını kaydeder; yaşam döngüsü durumlarını taslak, gönderilmiş,
  kabul edilmiş referans veya geçersiz kılınmış olarak işaretler.
- Siparişleri tam teklif numarası ve revizyonuyla ilişkilendirir; mevcut olduğu
  ölçüde müşteri, satır, miktar, fiyat, iskonto, toplam, ödeme koşulu, teslimat
  koşulu ve navlun bilgilerini karşılaştırır.
- Deterministik sonuçları `MATCHED`, `REVIEW`, `CRITICAL` veya `INCOMPLETE`
  durumlarına yönlendirir; bulanık eşleştirme veya yapay zekâ tabanlı eşleştirme
  kullanmaz.
- Şirket politikalarını, teklif etiketi şablonlarını, tarama geçmişini ve
  sipariş inceleme geçmişini yerel bir SQLite veritabanında tutar.
- Masaüstü uygulaması çalışırken dosyaları elle veya alt klasörleri taramayan
  izlenen klasörlerden alır.
- Windows masaüstü uygulamasıyla veya Rust komut satırı tarayıcısıyla
  kullanılabilir.

## Gizlilik

Commercial Preflight, desteklenen belgeleri Windows cihazında işler. Uygulamada
hesap, bulutta belge işleme hizmeti veya telemetri entegrasyonu yoktur. Kaynak
belgeler salt okunur açılır ve veritabanına kopyalanmaz.

Yerel veritabanı, ürünün ihtiyaç duyduğu yapılandırma ve operasyon verilerini
tutar: belge özetleri (hash), dosya adları ve kaynak yolları, politika/şablon
sürümleri, sonuçlar ve bulgular ile geçmiş karşılaştırmalarda kullanılan
yapılandırılmış teklif/sipariş değerleri. Belge gövdelerinin tamamını,
çıkarılmış PDF metnini, çalışma sayfası gövdelerini veya kaynak kanıt
alıntılarını saklamaz. Windows hesabını ve yerel veritabanını kuruluşunuzun veri
politikasına göre koruyun.

Müşteri belgelerini hiçbir zaman GitHub'da herkese açık bir issue'ya eklemeyin.
Güvenlik açıklarını özel olarak bildirmek için [SECURITY.md](SECURITY.md)
dosyasına bakın.

## Ekran görüntüleri

İlk yayın yükleyicisi yayımlandıktan sonra herkese açık ekran görüntüleri
eklenecektir.

> **Ekran görüntüsü yer tutucusu:** Sipariş Gelen Kutusu ve karşılaştırma durumu

> **Ekran görüntüsü yer tutucusu:** Teklif-sipariş fark incelemesi

> **Ekran görüntüsü yer tutucusu:** Teklif Kontrolü bulguları

## Windows kurulumu

### Hazır yayın yükleyicisi

Hazır bir sürüm yayımlandığında:

1. Bu deponun **Releases** sayfasını açın.
2. v0.1.0 Windows NSIS kurulum dosyasını (`*-setup.exe`) indirin.
3. İndirmenin beklediğiniz GitHub deposundan geldiğini doğrulayın ve yükleyiciyi
   kuruluşunuzun yazılım politikasına uygun şekilde çalıştırın.
4. Başlat menüsünden **Commercial Preflight** uygulamasını açın.

İlk sürümler kod imzalı olmayabilir. Windows veya kuruluşunuzdaki bir güvenlik
denetimi yükleyiciyi reddederse bu denetimi aşmayın; uygulamayı kaynak koddan
derleyin veya yöneticinizden sürümü incelemesini isteyin.

### Windows uygulamasını kaynak koddan derleme

Şunları kurun:

- Rust 1.88 veya üzeri;
- npm ile birlikte güncel bir Node.js 22 sürümü (veya üzeri);
- Windows PowerShell 5.1 veya üzeri; ve
- Microsoft C++ Build Tools ile WebView2 dahil
  [Tauri Windows önkoşulları](https://v2.tauri.app/start/prerequisites/).

Deponun yerel kopyasında:

```powershell
cd apps\desktop
npm ci
npm run tauri build
```

Yapılandırılmış NSIS paketi, depo kökündeki
`target\release\bundle\nsis\` dizininin altına yazılır. Geliştirme sürümünü
çalıştırmak için `apps\desktop` dizininde `npm run tauri dev` komutunu kullanın.

## Hızlı Başlangıç

1. Uygulamayı başlatın; tek seferlik şirket, para birimi ve politika kurulumunu
   tamamlayın.
2. **Teklifler** bölümünü açın, **Teklif Ekle** seçeneğiyle teklif dosyasını
   kaydedin. Sipariş eşleştirmesine uygun bir revizyonu **Gönderildi Olarak
   İşaretle** veya **Referans Olarak İşaretle** seçeneğiyle işaretleyin.
3. **Sipariş Gelen Kutusu** bölümünü açın, **Sipariş Ekle** seçeneğini kullanın
   ve müşteri siparişini seçin.
4. Sonucu açın: `MATCHED`, yapılandırılmış kontrollerin ticari bir fark
   bulmadığını; `REVIEW` ise bir veya daha fazla farkın bir kişi tarafından
   değerlendirilmesi gerektiğini belirtir.
5. Bir teklifi doğrudan etkin şirket politikasına göre kontrol etmek için
   **Araçlar → Teklif Kontrolü** bölümünü kullanın.

İlk kurulum, sonuç inceleme ve izlenen klasör ayarları için teknik olmayan
[Hızlı Başlangıç kılavuzuna](docs/QUICK_START.tr.md) bakın.

## Desteklenen biçimler

| Biçim | Mevcut v0.1 desteği |
| --- | --- |
| XLSX | Tanınan etiketleri ve tablo biçiminde satır kalemleri bulunan teklifler ve siparişler. Dosyalar, sınırlandırılmış Windows PowerShell/.NET çıkarıcısı ile salt okunur incelenir. |
| PDF | Ayrıştırıcının tanıdığı deterministik etiket/satır düzenine uyan, seçilebilir metin içeren teklifler ve siparişler. |

DOCX, eski XLS biçimi, yalnızca görüntü içeren PDF'ler ve rastgele PDF tablo
düzenleri desteklenmez. Desteklenmeyen veya güvenli biçimde işlenemeyen girdi,
temiz bir sonuç yerine `INCOMPLETE` döndürür.

## Mevcut sınırlamalar

- Desteklenen ürün hedefi Windows'tur; XLSX çıkarımı Windows
  PowerShell/.NET'e bağlıdır.
- PDF desteği bilinçli olarak dardır. OCR, şifreli PDF'ler, alışılmadık yazı tipi
  kodlamaları ve sezgisel tablo oluşturma uygulanmamıştır.
- Şablon tanıma, makine öğrenmesi veya bulanık çıkarım yerine yapılandırılmış
  etiketleri kullanır.
- İzlenen klasörler alt klasörleri taramaz ve yalnızca masaüstü uygulaması
  çalışırken kontrol edilir.
- v0.1'de bulut eşitleme, merkezi yönetim, kimlik doğrulama, Outlook/ERP
  entegrasyonu veya otomatik engelleme yoktur.
- `MATCHED` veya `PASS` sonucu yalnızca tamamlanabilen yapılandırılmış
  kontrolleri tanımlar. İnsan onayı yine gereklidir.
- Arayüzler, depolama şeması ve desteklenen belge düzenleri v0.1 serisi boyunca
  değişebilir.

## Komut satırı tarayıcısı

CLI, bir teklifi açıkça belirtilen JSON politikasıyla kontrol eder:

```powershell
cargo run --locked -- scan tests/fixtures/Project_Orion_Quote_v0.1.xlsx `
  --policy config/company_policy_orion.json `
  --as-of 2026-10-30
```

Makine tarafından işlenebilir çıktı için `--json` ekleyin. Çıkış kodu `0`,
analizin `PASS`, `REVIEW` veya `CRITICAL` ile tamamlandığını; çıkış kodu `2`
ise geçersiz girdi/yapılandırma ya da `INCOMPLETE` tarama bulunduğunu belirtir.

Depodaki tüm test dosyaları yalnızca sentetik veri içerir.

## Mimariye genel bakış

```text
Masaüstü arayüzü / CLI
       |
elle veya izlenen klasörden belge alımı
       |
sınırlandırılmış XLSX veya PDF ayrıştırma
       |
normalize edilmiş ticari belge
       |
politika kontrolleri veya teklif/sipariş karşılaştırması
       |
yerel SQLite geçmişi + masaüstü incelemesi
```

Rust çalışma alanı; biçim ayrıştırma, normalize edilmiş alan modelleri,
politika kuralları, belge alımı, karşılaştırma ve depolama sorumluluklarını
birbirinden ayırır. Tauri masaüstü arka ucu bu yetenekleri TypeScript kullanıcı
arayüzüne sunar. Python uygulaması yalnızca ilk XLSX davranışının referansı
olarak tutulur; üretim masaüstü uygulaması veya Rust CLI tarafından kullanılmaz.

Bileşen sınırları, veri akışları, güvenlik limitleri ve yerel kalıcılık
ayrıntıları için [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) dosyasına bakın.

## Geliştirme ve derleme

Rust çalışma alanını derleyin:

```powershell
cargo build --workspace --locked
```

Masaüstü ön yüzü bağımlılıklarını kurun ve doğrulayın:

```powershell
cd apps\desktop
npm ci
npm run typecheck
npm test
npm run build
```

Masaüstü uygulamasını geliştirme modunda çalıştırmak için
`npm run tauri dev` komutunu kullanın.

Sabitlenmiş Python referans uygulamasını çalıştırmak için sanal ortamda Python
3.11 veya üzerini kullanın ve `requirements.txt` dosyasını kurun.

## Testler

Yayın kontrollerini depo kökünden çalıştırın:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

Push-Location apps\desktop
npm ci
npm run typecheck
npm test
npm run build
Pop-Location

python -m unittest discover -s tests -p "test_*.py"
```

Regresyon testleri; sentetik XLSX ve PDF teklif kontrollerini, hatalı, kötü
amaçlı veya eksik girdileri, teklif/sipariş ilişkilendirmesi ve farklarını,
yerel kalıcılığı, izlenen klasörlerden belge alımını ve masaüstü
yerelleştirme/ilk kurulum davranışını kapsar.

## Katkı ve güvenlik

Geliştirme iş akışı ve test dosyası kuralları
[CONTRIBUTING.md](CONTRIBUTING.md) dosyasındadır. Güvenlik açıklarını, hassas
bilgi içeren ve herkese açık bir GitHub issue'su yerine
[SECURITY.md](SECURITY.md) dosyasındaki özel süreçle bildirin.

## Lisans

Commercial Preflight,
[Apache License 2.0](LICENSE) kapsamında lisanslanmıştır.
