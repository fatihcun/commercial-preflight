# Windows için Hızlı Başlangıç

English: [QUICK_START.md](QUICK_START.md)

Bu kılavuz, v0.1 masaüstü iş akışını anlatır. Commercial Preflight yerel
dosyalarla çalışır ve Commercial Preflight bulut hesabı gerektirmez.

## 1. Windows uygulamasını kurun

1. Projenin GitHub **Releases** sayfasını açın.
2. v0.1 Windows kurulum dosyasını (`*-setup.exe`) indirin.
3. Dosyanın beklediğiniz depodan geldiğini doğrulayın ve kuruluşunuzun yazılım
   politikasına uygun şekilde çalıştırın.
4. Başlat menüsünden **Commercial Preflight** uygulamasını açın.

Henüz bir yükleyici yayımlanmadıysa yöneticinizden veya geliştiricinizden, ana
README dosyasındaki yönergelerle Windows uygulamasını derlemesini isteyin.

## 2. İlk şirket kurulumunu tamamlayın

İlk açılışta **İlk Kurulum** görüntülenir.

1. Şirket adınızı ve varsayılan para birimini girin.
2. Azami iskonto, azami ödeme vadesi, isteğe bağlı asgari marj ve teklif
   geçerlilik süresi dahil olmak üzere başlangıç ticari sınırlarını inceleyin.
3. Etkinleştirilecek kontrolleri seçin.
4. Özeti inceleyin ve **Kurulumu Tamamla** seçeneğini kullanın.

Bu ayarlar ilgili Windows hesabında saklanır. Daha sonra **Politikalar**,
**Şablonlar** ve **Ayarlar** bölümlerinden incelenebilir.

## 3. Teklif ekleyin ve kaydedin

1. **Teklifler** bölümünü açın.
2. **Teklif Ekle** seçeneğini kullanın ve desteklenen bir XLSX dosyası veya
   seçilebilir metin içeren bir PDF dosyası seçin.
3. Uygulamanın gösterdiği teklif numarası ile revizyonun beklediğiniz değerler
   olduğunu doğrulayın.
4. Müşteriye gönderilen revizyon için **Gönderildi Olarak İşaretle** seçeneğini;
   sürecinizde karşılaştırma referansı kabul edilen revizyon için **Referans
   Olarak İşaretle** seçeneğini kullanın.

Taslak teklifler kaydedilir ancak otomatik sipariş eşleştirmesinde kullanılmaz.
Her revizyonu ayrı bir kayıt olarak tutun.

## 4. Müşteri siparişi ekleyin

1. **Sipariş Gelen Kutusu** bölümünü açın.
2. **Sipariş Ekle** seçeneğini kullanın ve müşteri siparişi dosyasını seçin.
3. Commercial Preflight, siparişteki teklif numarası ile revizyonu okur ve
   eşleştirmeye uygun kayıtlı bir teklif arar.
4. Kesin ilişkilendirme güvenli biçimde yapılamazsa siparişi açın ve elle
   ilişkilendirmek için **Teklif Seç** seçeneğini kullanın. Bu seçim yerel
   geçmişe kaydedilir.

## 5. `MATCHED` ve `REVIEW` sonuçlarını inceleyin

- `MATCHED`, yapılandırılmış karşılaştırmanın sipariş ile ilişkili teklif
  arasında ticari fark bulmadığını belirtir.
- `REVIEW`, en az bir farkın bir kişi tarafından değerlendirilmesi gerektiğini
  belirtir. Teklif ve sipariş değerlerini alan veya satır bazında karşılaştırmak
  için siparişi açın.
- `CRITICAL`, kritik olarak yapılandırılmış bir farkı belirtir.
- `INCOMPLETE`, belge, ilişkilendirme veya karşılaştırmanın güvenli biçimde
  tamamlanamadığını belirtir. Bu bir eşleşme değildir.

Karşılaştırmada gösterilen müşteri, teklif revizyonu, miktar, fiyat, iskonto,
toplam ve koşulları kontrol edin. **İncelendi Olarak İşaretle**, bir kişinin
kaydı incelediğini belirtir; karşılaştırma sonucunu `MATCHED` olarak değiştirmez.

## 6. Teklif Kontrolü'nü kullanın

Teklif Kontrolü, teklif-sipariş karşılaştırmasından ayrı bir işlevdir.

1. **Araçlar → Teklif Kontrolü** bölümünü açın.
2. Desteklenen bir teklifi pencereye bırakın veya **Teklif Seç** seçeneğini
   kullanın.
3. Özeti inceleyin; kanıt ve beklenen değerleri görmek için **Bulguları
   Görüntüle** seçeneğini kullanın.
4. Kaynak dosyayı normal uygulamasında düzeltin ve yeniden kontrol edin.

Commercial Preflight kaynak dosyayı okur ancak değiştirmez.

## 7. İzlenen klasörleri yapılandırın

İzlenen klasörler, dosya seçici kullanmadan gelen dosyaların kaydedilmesini
sağlar.

1. Biri teklifler, diğeri müşteri siparişleri için olmak üzere iki ayrı yerel
   klasör oluşturun.
2. **Ayarlar → Uygulama** bölümünü açın.
3. **Otomatik klasör alımı** altında **Teklif alım klasörü** ve **Sipariş alım
   klasörü** seçeneklerini belirleyin.
4. **Klasörleri Kaydet** seçeneğini kullanın.
5. Commercial Preflight çalışırken desteklenen PDF veya XLSX dosyalarını ilgili
   klasöre kopyalayın.

Klasör izleme alt klasörleri taramaz. Bir dosya ancak art arda iki kontrolde
değişmeden kaldıktan sonra işlenir; bu nedenle görünmesi birkaç saniye
sürebilir. Kilitli dosyalar ertelenir, aynı içeriğe sahip dosyalar yeniden
işlenmez ve kaynak dosyalar hiçbir zaman değiştirilmez. İlgisiz veya hassas
dosyaları izlenen klasörlere koymayın.

## Güvenli biçimde yardım alma

GitHub'daki herkese açık issue'larda yalnızca sentetik örnekler kullanın. Gerçek
ticari bilgi içeren müşteri teklifi, sipariş, politika, veritabanı veya ekran
görüntülerini hiçbir zaman yüklemeyin. Hassas güvenlik bildirimleri için
[SECURITY.md](../SECURITY.md) dosyasındaki süreci izleyin.
