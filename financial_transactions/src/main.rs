use axum::{
    body::Body,
    http::{header, Response, StatusCode},
    routing::{get, post},
    Router,
};
use rand::Rng;

// SOAP/WSDL Financial Transactions Service
// Receives credit card info and returns Yes (90%) or No (10%)

async fn process_transaction(body: String) -> Response<Body> {
    // Parse the SOAP request (simple string-based parsing for prototype)
    let has_username = body.contains("<ft:UserName>") || body.contains("<UserName>");
    let has_card = body.contains("<ft:CreditCardNumber>") || body.contains("<CreditCardNumber>");
    let has_expiry = body.contains("<ft:ExpirationDate>") || body.contains("<ExpirationDate>");
    let has_security = body.contains("<ft:SecurityCode>") || body.contains("<SecurityCode>");

    if !has_username || !has_card || !has_expiry || !has_security {
        let fault_response = r#"<?xml version="1.0" encoding="UTF-8"?>
<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/">
  <soap:Body>
    <soap:Fault>
      <faultcode>soap:Client</faultcode>
      <faultstring>Missing required fields: UserName, CreditCardNumber, ExpirationDate, SecurityCode</faultstring>
    </soap:Fault>
  </soap:Body>
</soap:Envelope>"#;
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header(header::CONTENT_TYPE, "text/xml; charset=utf-8")
            .body(Body::from(fault_response))
            .unwrap();
    }

    // 90% chance of approval, 10% chance of rejection
    let mut rng = rand::thread_rng();
    let approved = rng.gen_range(0..100) < 90;

    let soap_response = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"
               xmlns:ft="http://marketplace.example.com/financial">
  <soap:Body>
    <ft:ProcessTransactionResponse>
      <Approved>{}</Approved>
      <Message>{}</Message>
    </ft:ProcessTransactionResponse>
  </soap:Body>
</soap:Envelope>"#,
        approved,
        if approved {
            "Transaction approved"
        } else {
            "Transaction declined"
        }
    );

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/xml; charset=utf-8")
        .body(Body::from(soap_response))
        .unwrap()
}

// GET /financial/wsdl - serves the WSDL definition
async fn get_wsdl() -> Response<Body> {
    let wsdl = r#"<?xml version="1.0" encoding="UTF-8"?>
<definitions name="FinancialTransactions"
             targetNamespace="http://marketplace.example.com/financial"
             xmlns="http://schemas.xmlsoap.org/wsdl/"
             xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap/"
             xmlns:tns="http://marketplace.example.com/financial"
             xmlns:xsd="http://www.w3.org/2001/XMLSchema">

  <types>
    <xsd:schema targetNamespace="http://marketplace.example.com/financial">
      <xsd:element name="ProcessTransaction">
        <xsd:complexType>
          <xsd:sequence>
            <xsd:element name="UserName" type="xsd:string"/>
            <xsd:element name="CreditCardNumber" type="xsd:string"/>
            <xsd:element name="ExpirationDate" type="xsd:string"/>
            <xsd:element name="SecurityCode" type="xsd:string"/>
          </xsd:sequence>
        </xsd:complexType>
      </xsd:element>
      <xsd:element name="ProcessTransactionResponse">
        <xsd:complexType>
          <xsd:sequence>
            <xsd:element name="Approved" type="xsd:boolean"/>
            <xsd:element name="Message" type="xsd:string"/>
          </xsd:sequence>
        </xsd:complexType>
      </xsd:element>
    </xsd:schema>
  </types>

  <message name="ProcessTransactionRequest">
    <part name="parameters" element="tns:ProcessTransaction"/>
  </message>
  <message name="ProcessTransactionResponse">
    <part name="parameters" element="tns:ProcessTransactionResponse"/>
  </message>

  <portType name="FinancialTransactionsPortType">
    <operation name="ProcessTransaction">
      <input message="tns:ProcessTransactionRequest"/>
      <output message="tns:ProcessTransactionResponse"/>
    </operation>
  </portType>

  <binding name="FinancialTransactionsBinding" type="tns:FinancialTransactionsPortType">
    <soap:binding style="document" transport="http://schemas.xmlsoap.org/soap/http"/>
    <operation name="ProcessTransaction">
      <soap:operation soapAction="http://marketplace.example.com/financial/ProcessTransaction"/>
      <input><soap:body use="literal"/></input>
      <output><soap:body use="literal"/></output>
    </operation>
  </binding>

  <service name="FinancialTransactionsService">
    <port name="FinancialTransactionsPort" binding="tns:FinancialTransactionsBinding">
      <soap:address location="http://localhost:8085/financial/transaction"/>
    </port>
  </service>
</definitions>"#;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/xml; charset=utf-8")
        .body(Body::from(wsdl))
        .unwrap()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bind_addr = std::env::var("FINANCIAL_TX_BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8085".to_string());

    let app = Router::new()
        .route("/financial/transaction", post(process_transaction))
        .route("/financial/wsdl", get(get_wsdl));

    println!(
        "Financial Transactions SOAP/WSDL service listening on {}",
        bind_addr
    );
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
